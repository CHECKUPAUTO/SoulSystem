"""ZeroBot FastAPI server.

Provides:
- POST /chat — Send a message to the agent
- POST /upload — Upload a binary for analysis
- POST /session/authorize — Authorize a binary hash
- GET /health — Health check
- GET / — Swagger/OpenAPI documentation
"""

from __future__ import annotations

import hashlib
import logging
import os
import shutil
import uuid
from pathlib import Path
from typing import Any, Optional

import yaml
from fastapi import FastAPI, File, HTTPException, UploadFile
from langchain_openai import ChatOpenAI
from langchain_ollama import ChatOllama

from agent.orchestrator import ZeroBotAgent
from api.schemas import (
    AuthorizeRequest,
    AuthorizeResponse,
    ChatRequest,
    ChatResponse,
    ErrorResponse,
    UploadResponse,
)
from ethics.filters import EthicalFilter
from knowledge.retriever import ZeroBotRetriever
from sandbox.container_manager import ContainerManager
from tools import (
    BinaryAnalysisTool,
    CrashAnalyzerTool,
    DisassembleFunctionTool,
    ExploitTemplatesTool,
    FuzzingHarnessTool,
    WebSearchTool,
)

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger(__name__)

# ── Configuration ─────────────────────────────────────────────────
def load_config() -> dict[str, Any]:
    config_path = os.environ.get("ZEROBOT_CONFIG", "/zerobot/config.yaml")
    try:
        with open(config_path) as f:
            return yaml.safe_load(f) or {}
    except FileNotFoundError:
        logger.warning("Config not found at %s", config_path)
        return {}

config = load_config()

# ── LLM Setup ─────────────────────────────────────────────────────
def create_llm():
    """Create the LLM based on configuration."""
    model_config = config.get("models", {}).get("llm", {})
    provider = model_config.get("provider", "openai")

    if provider == "ollama":
        return ChatOllama(
            base_url=model_config.get("ollama_base_url", "http://ollama:11434"),
            model=model_config.get("ollama_model", "mistral:7b-instruct"),
            temperature=model_config.get("temperature", 0.1),
            num_predict=model_config.get("max_tokens", 4096),
        )
    else:
        return ChatOpenAI(
            model=model_config.get("model_name", "gpt-4"),
            temperature=model_config.get("temperature", 0.1),
            max_tokens=model_config.get("max_tokens", 4096),
        )


def create_agent() -> ZeroBotAgent:
    """Create the ZeroBot agent with all components."""
    llm = create_llm()

    # Tools
    tools = [
        BinaryAnalysisTool(),
        DisassembleFunctionTool(),
        FuzzingHarnessTool(),
        CrashAnalyzerTool(),
        ExploitTemplatesTool(),
        WebSearchTool(),
    ]

    # RAG
    retriever = None
    try:
        rag_config = config.get("rag", {})
        persist_path = config.get("paths", {}).get(
            "chromadb_persist", "/zerobot/data/chromadb"
        )
        model_name = config.get("models", {}).get("embedding", {}).get(
            "model_name", "BAAI/bge-large-en-v1.5"
        )
        retriever = ZeroBotRetriever(
            chromadb_path=persist_path,
            embedding_model=model_name,
            k=rag_config.get("retrieval_k", 5),
            score_threshold=rag_config.get("score_threshold", 0.5),
        )
    except Exception as e:
        logger.warning("RAG initialization failed (non-fatal): %s", e)

    # Ethical filter
    allowed_hashes = config.get("allowed_hashes", [])
    ethical_filter = EthicalFilter(allowed_hashes=allowed_hashes)

    return ZeroBotAgent(
        llm=llm,
        tools=tools,
        retriever=retriever,
        ethical_filter=ethical_filter,
    )

# ── FastAPI App ───────────────────────────────────────────────────
app = FastAPI(
    title="ZeroBot — Vulnerability Research Assistant",
    description="API for the ZeroBot zero-day vulnerability research assistant. "
    "Provides static analysis, disassembly, fuzzing harness generation, "
    "crash analysis, and educational exploit templates. "
    "All operations require authorized targets only.",
    version="1.0.0",
    contact={
        "name": "ZeroBot Team",
        "url": "https://github.com/zerobot",
    },
    license_info={
        "name": "Educational Use Only",
    },
)

# Session state
_sessions: dict[str, dict[str, Any]] = {}
_agent: Optional[ZeroBotAgent] = None
_container_manager = ContainerManager()


@app.on_event("startup")
async def startup():
    """Initialize the agent on server start."""
    global _agent
    try:
        _agent = create_agent()
        logger.info("ZeroBot agent initialized successfully")
    except Exception as e:
        logger.error("Failed to initialize agent: %s", e)
        _agent = None


def _get_or_create_session(session_id: str) -> dict[str, Any]:
    """Get or create a session state dictionary."""
    if session_id not in _sessions:
        _sessions[session_id] = {
            "id": session_id,
            "authorized_hashes": set(),
            "files": {},
            "created_at": __import__("datetime").datetime.now().isoformat(),
        }
    return _sessions[session_id]


# ── Endpoints ─────────────────────────────────────────────────────

@app.get("/", include_in_schema=False)
async def root():
    """Redirect to Swagger docs."""
    return {"message": "ZeroBot API — See /docs for Swagger documentation"}


@app.get("/health", tags=["System"])
async def health():
    """Health check endpoint."""
    agent_ok = _agent is not None
    return {
        "status": "ok" if agent_ok else "degraded",
        "agent_initialized": agent_ok,
        "sessions_active": len(_sessions),
    }


@app.post("/chat", response_model=ChatResponse, tags=["Chat"])
async def chat(request: ChatRequest):
    """Send a message to the ZeroBot agent."""
    if _agent is None:
        raise HTTPException(status_code=503, detail="Agent not initialized")

    session = _get_or_create_session(request.session_id)

    result = _agent.process_message(
        message=request.message,
        session_id=request.session_id,
    )

    return ChatResponse(
        response=result.get("response", ""),
        session_id=request.session_id,
        blocked=result.get("blocked", False),
        classification=result.get("classification", "unknown"),
        rag_context_used=result.get("rag_context_used", False),
        tool_calls=result.get("tool_calls", []),
    )


@app.post("/upload", response_model=UploadResponse, tags=["Binary"])
async def upload_binary(
    file: UploadFile = File(...),
    session_id: str = "default",
):
    """Upload a binary file for analysis.

    The binary is stored in a sandboxed directory, hashed, and checked
    against the allowlist. If not authorized, a superficial educational
    analysis may still be possible.
    """
    # Validate file
    if not file.filename:
        raise HTTPException(status_code=400, detail="No file provided")

    max_size = config.get("session", {}).get("max_binary_size_mb", 100)
    contents = await file.read()
    if len(contents) > max_size * 1024 * 1024:
        raise HTTPException(
            status_code=413,
            detail=f"File exceeds maximum size of {max_size}MB",
        )

    # Store in sandbox
    session = _get_or_create_session(session_id)
    sandbox_dir = Path(config.get("paths", {}).get("binary_store", "/zerobot/sessions"))
    sandbox_dir.mkdir(parents=True, exist_ok=True)

    # Unique filename to prevent collisions
    unique_name = f"{uuid.uuid4().hex}_{file.filename}"
    store_path = sandbox_dir / unique_name

    with open(store_path, "wb") as f:
        f.write(contents)

    # Compute hash
    sha256_hash = hashlib.sha256(contents).hexdigest()

    # Check allowlist
    container_mgr = ContainerManager()
    allowed = container_mgr.is_hash_allowed(sha256_hash)

    # Also check session-level authorization
    if not allowed and sha256_hash in session.get("authorized_hashes", set()):
        allowed = True

    # Store in session state
    session["files"][sha256_hash] = {
        "path": str(store_path),
        "filename": file.filename,
        "authorized": allowed,
    }

    if allowed:
        message = (
            f"Binary '{file.filename}' accepted (hash authorized). "
            f"Ready for analysis. SHA256: {sha256_hash[:16]}..."
        )
    else:
        message = (
            f"Binary '{file.filename}' stored but NOT authorized for deep analysis. "
            f"SHA256: {sha256_hash[:16]}... "
            f"Use /session/authorize with this hash if you have authorization."
        )

    return UploadResponse(
        filename=file.filename,
        stored_path=str(store_path),
        sha256=sha256_hash,
        authorized=allowed,
        message=message,
    )


@app.post("/session/authorize", response_model=AuthorizeResponse, tags=["Session"])
async def authorize_binary(request: AuthorizeRequest):
    """Authorize a binary hash for analysis in a session."""
    session = _get_or_create_session(request.session_id)
    session["authorized_hashes"].add(request.sha256)

    # Also update the ethical filter if agent is running
    if _agent and _agent.ethical_filter:
        _agent.ethical_filter.add_to_allowlist(request.sha256)

    return AuthorizeResponse(
        sha256=request.sha256,
        authorized=True,
        message=f"Hash {request.sha256[:16]}... authorized for session {request.session_id}",
    )


@app.get("/session/{session_id}", tags=["Session"])
async def get_session_info(session_id: str):
    """Get information about a session."""
    session = _get_or_create_session(session_id)
    return {
        "session_id": session_id,
        "authorized_hashes": list(session.get("authorized_hashes", set())),
        "uploaded_files": list(session.get("files", {}).keys()),
        "created_at": session.get("created_at", ""),
    }
