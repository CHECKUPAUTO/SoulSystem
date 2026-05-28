"""Base class for ZeroBot LangChain tools.

All tools inherit from BaseTool and share:
- Docker container execution via ContainerManager
- Structured input/output schemas
- Error handling and timeouts
"""

from __future__ import annotations

import logging
from abc import ABC, abstractmethod
from typing import Any, Optional

from langchain_core.tools import BaseTool as LangChainBaseTool
from pydantic import BaseModel, Field

from sandbox.container_manager import ContainerManager, ToolboxResult

logger = logging.getLogger(__name__)


class BaseAnalysisTool(LangChainBaseTool, ABC):
    """Abstract base tool for ZeroBot analysis operations."""

    container_manager: ContainerManager = Field(default_factory=ContainerManager)

    def __init__(
        self,
        container_manager: Optional[ContainerManager] = None,
        **kwargs,
    ):
        super().__init__(**kwargs)
        self.container_manager = container_manager or ContainerManager()

    def run_in_toolbox(
        self,
        command: str | list[str],
        binary_path: Optional[str] = None,
        timeout: Optional[int] = None,
    ) -> ToolboxResult:
        """Execute a command in the sandboxed toolbox container."""
        return self.container_manager.run_tool(
            command=command,
            binary_path=binary_path,
            timeout=timeout,
        )

    def parse_error_result(self, error: str) -> dict[str, Any]:
        """Return a standardized error dictionary."""
        return {"error": error, "success": False}
