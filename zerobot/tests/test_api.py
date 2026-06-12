"""Unit tests for the ZeroBot FastAPI server.

Tests endpoints with mocked agent to avoid requiring LLM connections.
"""

import hashlib
from unittest.mock import MagicMock, patch

import pytest
from httpx import ASGITransport, AsyncClient

from api.server import app, _sessions


@pytest.fixture
def client():
    """Create an async test client with clean session state."""
    _sessions.clear()
    transport = ASGITransport(app=app)
    return AsyncClient(transport=transport, base_url="http://test")


@pytest.fixture
def mock_agent():
    """Create a mock ZeroBot agent."""
    agent = MagicMock()
    agent.process_message.return_value = {
        "response": "This is a test response.",
        "blocked": False,
        "classification": "educational",
        "session_id": "test_session",
        "rag_context_used": False,
        "tool_calls": [],
    }
    agent.ethical_filter = MagicMock()
    return agent


class TestHealth:
    async def test_health_endpoint(self, client):
        """Health check should return ok status."""
        response = await client.get("/health")
        assert response.status_code == 200
        data = response.json()
        assert "status" in data


class TestChat:
    async def test_chat_returns_503_when_no_agent(self, client):
        """Without initialized agent, /chat should return 503."""
        response = await client.post(
            "/chat",
            json={
                "message": "Help me understand buffer overflows",
                "session_id": "test_session",
            },
        )
        assert response.status_code == 503
        assert "Agent not initialized" in response.json()["detail"]

    async def test_chat_with_agent(self, client, mock_agent):
        """With mock agent, /chat should return the mock response."""
        with patch("api.server._agent", mock_agent):
            response = await client.post(
                "/chat",
                json={
                    "message": "Analyze this binary",
                    "session_id": "test_session",
                },
            )
        assert response.status_code == 200
        data = response.json()
        assert data["response"] == "This is a test response."
        assert data["session_id"] == "test_session"

    async def test_chat_sessions_isolated(self, client, mock_agent):
        """Different session IDs should persist independently."""
        with patch("api.server._agent", mock_agent):
            for sid in ["session_a", "session_b"]:
                response = await client.post(
                    "/chat",
                    json={"message": "Hello", "session_id": sid},
                )
                assert response.status_code == 200


class TestUpload:
    async def test_upload_binary(self, client):
        """Uploading a binary should return hash and filename."""
        binary_content = b"\x7fELF\x02\x01\x01\x00" + b"\x00" * 100

        response = await client.post(
            "/upload?session_id=test_session",
            files={"file": ("test.elf", binary_content, "application/octet-stream")},
        )

        assert response.status_code == 200
        data = response.json()
        assert "sha256" in data
        assert data["filename"] == "test.elf"
        assert "authorized" in data

    async def test_upload_no_file_returns_422(self, client):
        """Upload without a file should return 422."""
        response = await client.post("/upload")
        assert response.status_code == 422


class TestAuthorize:
    async def test_authorize_hash(self, client):
        """Authorizing a hash should succeed."""
        test_hash = "a" * 64

        response = await client.post(
            "/session/authorize",
            json={"sha256": test_hash, "session_id": "test_session"},
        )

        assert response.status_code == 200
        data = response.json()
        assert data["sha256"] == test_hash
        assert data["authorized"] is True

    async def test_session_info(self, client):
        """Session info should list authorized hashes."""
        test_hash = "b" * 64
        await client.post(
            "/session/authorize",
            json={"sha256": test_hash, "session_id": "info_session"},
        )

        response = await client.get("/session/info_session")
        assert response.status_code == 200
        data = response.json()
        assert data["session_id"] == "info_session"
