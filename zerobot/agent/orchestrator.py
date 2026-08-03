"""ZeroBot agent orchestrator.

Wires together the LLM, tools, RAG retriever, and ethical filters into
a LangChain agent executor.
"""

from __future__ import annotations

import logging
from typing import Any, Optional

from langchain_classic.agents import AgentExecutor, create_tool_calling_agent
from langchain_classic.memory import ConversationBufferMemory
from langchain_core.prompts import ChatPromptTemplate, MessagesPlaceholder
from langchain_core.runnables import RunnableConfig
from langchain_core.tools import BaseTool

from agent.prompts import SYSTEM_PROMPT, RAG_CONTEXT_TEMPLATE
from ethics.filters import EthicalFilter, IntentClassification
from knowledge.retriever import ZeroBotRetriever
from avid_bridge import (
    search_memory,
    get_context as get_soulsystem_context,
    store_static_analysis,
    store_disassembly,
    store_crash_analysis,
    store_exploit_template,
    store_fuzz_harness,
)

logger = logging.getLogger(__name__)


class ZeroBotAgent:
    """Orchestrator for the ZeroBot vulnerability research agent."""

    def __init__(
        self,
        llm: Any,  # LangChain BaseLanguageModel
        tools: list[BaseTool],
        retriever: Optional[ZeroBotRetriever] = None,
        ethical_filter: Optional[EthicalFilter] = None,
        session_id: str = "default",
        rag_k: int = 5,
    ):
        self.llm = llm
        self.tools = {tool.name: tool for tool in tools}
        self.tool_list = tools
        self.retriever = retriever
        self.ethical_filter = ethical_filter or EthicalFilter()
        self.session_id = session_id
        self.rag_k = rag_k

        # Conversation memory per session
        self._memory: dict[str, ConversationBufferMemory] = {}

    def _get_memory(self, session_id: str) -> ConversationBufferMemory:
        """Get or create conversation memory for a session."""
        if session_id not in self._memory:
            self._memory[session_id] = ConversationBufferMemory(
                memory_key="chat_history",
                return_messages=True,
            )
        return self._memory[session_id]

    def _build_prompt(self) -> ChatPromptTemplate:
        """Build the agent's chat prompt template."""
        return ChatPromptTemplate.from_messages([
            ("system", SYSTEM_PROMPT),
            MessagesPlaceholder(variable_name="chat_history"),
            ("human", "{input}"),
            MessagesPlaceholder(variable_name="agent_scratchpad"),
        ])

    def _get_relevant_context(self, query: str) -> str:
        """Retrieve relevant context from the knowledge base AND shared memory."""
        parts = []

        # 1. RAG local (documents ingérés)
        if self.retriever is not None:
            try:
                docs = self.retriever.invoke(query)
                if docs:
                    context_parts = []
                    for i, doc in enumerate(docs[:self.rag_k]):
                        source = doc.metadata.get("source", "unknown")
                        context_parts.append(
                            f"[K{i + 1}] (Source: {source})\n{doc.page_content[:500]}"
                        )
                    if context_parts:
                        parts.append("## Knowledge Base\n" + "\n\n".join(context_parts))
            except Exception as e:
                logger.warning("RAG retrieval failed: %s", e)

        # 2. Mémoire partagée AVID/SoulSystem
        try:
            soul_ctx = get_soulsystem_context(query, limit=3)
            if soul_ctx:
                parts.append(f"## AVID Shared Memory\n{soul_ctx}")
        except Exception as e:
            logger.warning("SoulSystem context retrieval failed: %s", e)

        if not parts:
            return ""

        return RAG_CONTEXT_TEMPLATE.format(context="\n\n---\n\n".join(parts))

    def process_message(
        self,
        message: str,
        session_id: Optional[str] = None,
    ) -> dict[str, Any]:
        """Process a user message through the full agent pipeline.

        Steps:
        1. Ethical pre-filter
        2. RAG context retrieval
        3. LLM agent execution with tools
        4. Ethical post-filter
        """
        sid = session_id or self.session_id

        # ── 1. Ethical pre-filter ───────────────────────────────────
        filter_result = self.ethical_filter.pre_generation_filter(message)
        if filter_result.blocked:
            logger.info(
                "Session %s: Message blocked by ethical filter (%s)",
                sid, filter_result.classification.value,
            )
            return {
                "response": filter_result.reason,
                "blocked": True,
                "classification": filter_result.classification.value,
                "session_id": sid,
                "tool_calls": [],
            }

        # ── 2. RAG context ─────────────────────────────────────────
        rag_context = self._get_relevant_context(message)

        # ── 3. Build and invoke agent ───────────────────────────────
        memory = self._get_memory(sid)
        prompt = self._build_prompt()

        # Create the agent
        agent = create_tool_calling_agent(self.llm, self.tool_list, prompt)

        agent_executor = AgentExecutor(
            agent=agent,
            tools=self.tool_list,
            memory=memory,
            verbose=True,
            handle_parsing_errors=True,
            max_iterations=10,
            early_stopping_method="generate",
        )

        # Build the input with RAG context
        agent_input = message
        if rag_context:
            agent_input = f"{rag_context}\n\n## User Query\n{message}"

        try:
            response = agent_executor.invoke(
                {"input": agent_input},
                config=RunnableConfig(run_name=f"zerobot_{sid}"),
            )
            output = response.get("output", "I couldn't process that request.")
        except Exception as e:
            logger.error("Agent execution error: %s", e, exc_info=True)
            output = f"I encountered an error while processing your request: {e}"

        # ── 4. Auto-store results in SoulSystem/AVID memory ──────
        # Après chaque analyse, les résultats sont persistés dans la
        # mémoire partagée pour que les autres composants puissent les
        # consommer (AVID, SoulSystem, autres agents).
        if not filter_result.blocked and len(output) > 100:
            # Tag simple pour retrouver l'analyse
            binary_tag = "unknown"
            if "Binary:" in output:
                for line in output.split("\n"):
                    if line.startswith("Binary:"):
                        binary_tag = line.split(":", 1)[1].strip().split()[0]

            analysis_type = "zerobot_analysis"
            if "backtrace" in output.lower() or "crash" in output.lower():
                analysis_type = "crash"
            elif "exploit" in output.lower() or "payload" in output.lower():
                analysis_type = "exploit"
            elif "harness" in output.lower() or "fuzz" in output.lower():
                analysis_type = "fuzz"
            elif "disassembly" in output.lower() or "assembly" in output.lower():
                analysis_type = "disasm"

            try:
                store_static_analysis(binary_tag, output[:2000], sha256="")
            except Exception as e:
                logger.debug("SoulSystem auto-store: %s", e)

        # ── 5. Ethical post-filter ────────────────────────────────
        post_result = self.ethical_filter.post_generation_filter(output)
        if post_result.blocked:
            output = (
                "I generated a response that contained code patterns requiring "
                "additional review. Let me rephrase:\n\n"
                "The analysis results are available, but I need to ensure all "
                "shared code remains educational. Please ask for specific "
                "conceptual explanations rather than executable payloads."
            )

        return {
            "response": output,
            "blocked": False,
            "classification": filter_result.classification.value,
            "session_id": sid,
            "rag_context_used": bool(rag_context),
            "tool_calls": [],  # Populated by verbose callback in production
        }

    def get_tool(self, name: str) -> Optional[BaseTool]:
        """Get a registered tool by name."""
        return self.tools.get(name)
