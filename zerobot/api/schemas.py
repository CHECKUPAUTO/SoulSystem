"""Pydantic schemas for the ZeroBot API."""

from __future__ import annotations

from typing import Any, Optional

from pydantic import BaseModel, Field


class ChatRequest(BaseModel):
    """Request body for the /chat endpoint."""

    message: str = Field(..., description="User message to the agent")
    session_id: str = Field(default="default", description="Session identifier")


class ChatResponse(BaseModel):
    """Response from the /chat endpoint."""

    response: str = Field(..., description="Agent's response text")
    session_id: str = Field(..., description="Session identifier")
    blocked: bool = Field(default=False, description="Whether the request was blocked")
    classification: str = Field(default="unknown", description="Intent classification")
    rag_context_used: bool = Field(default=False)
    tool_calls: list[dict[str, Any]] = Field(default_factory=list)


class UploadResponse(BaseModel):
    """Response from the /upload endpoint."""

    filename: str = Field(..., description="Original filename")
    stored_path: str = Field(..., description="Path in the sandbox")
    sha256: str = Field(..., description="SHA256 hash of the binary")
    authorized: bool = Field(
        ..., description="Whether the binary is authorized for analysis"
    )
    message: str = Field(..., description="Human-readable status message")


class AuthorizeRequest(BaseModel):
    """Request body for the /session/authorize endpoint."""

    sha256: str = Field(..., description="SHA256 hash to authorize")
    session_id: str = Field(default="default", description="Session identifier")


class AuthorizeResponse(BaseModel):
    """Response from the /session/authorize endpoint."""

    sha256: str = Field(..., description="SHA256 hash that was authorized")
    authorized: bool = Field(default=True)
    message: str = Field(..., description="Human-readable status message")


class ErrorResponse(BaseModel):
    """Standard error response."""

    detail: str = Field(..., description="Error description")
    error_code: str = Field(default="unknown_error")
