"""Binary static analysis tool.

Runs file, checksec, strings, and import/export analysis on a binary
inside the sandboxed toolbox container.
"""

from __future__ import annotations

import json
import logging
import shlex
from typing import Any, Optional

from pydantic import BaseModel, Field

from tools.base import BaseAnalysisTool

logger = logging.getLogger(__name__)


class BinaryAnalysisInput(BaseModel):
    """Input schema for binary analysis."""

    binary_path: str = Field(
        description="Path to the binary file to analyze (must be in the sandbox)"
    )


class BinaryAnalysisTool(BaseAnalysisTool):
    """Analyze a binary's properties: architecture, protections, imports/exports."""

    name: str = "binary_analysis"
    description: str = (
        "Perform static analysis on a binary file. Returns architecture, "
        "security protections (PIE, NX, canary, RELRO, FORTIFY), linked "
        "libraries, and imported/exported symbols. The binary must already "
        "be uploaded to the sandbox."
    )
    args_schema: type[BaseModel] = BinaryAnalysisInput

    def _run(self, binary_path: str) -> dict[str, Any]:
        """Execute binary analysis."""
        result: dict[str, Any] = {"binary": binary_path, "success": False}

        # ── file(1) — identify file type ────────────────────────────
        file_res = self.run_in_toolbox(
            ["file", "-b", binary_path],
            binary_path=binary_path,
        )
        if file_res.error:
            result["error"] = file_res.error
            return result

        result["file_type"] = file_res.stdout

        # ── checksec — security protections ─────────────────────────
        checksec_res = self.run_in_toolbox(
            ["checksec", "--file=" + binary_path, "--output=json"],
            binary_path=binary_path,
        )
        protections: dict[str, Any] = {}
        if checksec_res.exit_code == 0 and checksec_res.stdout:
            try:
                protections = json.loads(checksec_res.stdout)
            except json.JSONDecodeError:
                protections["raw"] = checksec_res.stdout
        else:
            protections["note"] = "checksec not available or produced no output"
        result["protections"] = protections

        # ── strings — extract printable strings ─────────────────────
        strings_res = self.run_in_toolbox(
            ["strings", "-n", "8", binary_path],
            binary_path=binary_path,
        )
        if strings_res.exit_code == 0 and strings_res.stdout:
            result["strings_sample"] = strings_res.stdout.split("\n")[:50]
            result["strings_count"] = len(strings_res.stdout.split("\n"))

        # ── imports / exports via nm or rabin2 ──────────────────────
        imports: list[str] = []
        exports: list[str] = []

        # Try rabin2 first
        rabin2_res = self.run_in_toolbox(
            ["rabin2", "-Ss", binary_path],
            binary_path=binary_path,
        )
        if rabin2_res.exit_code == 0 and rabin2_res.stdout:
            for line in rabin2_res.stdout.split("\n"):
                # Parse rabin2 output: address, type, name
                parts = line.strip().split()
                if len(parts) >= 3:
                    sym_type = parts[1]
                    sym_name = parts[2]
                    if "IMPORT" in sym_type.upper():
                        imports.append(sym_name)
                    elif "EXPORT" in sym_type.upper() or "FUNC" in sym_type.upper():
                        exports.append(sym_name)

        # Fallback to nm -D
        if not imports and not exports:
            nm_res = self.run_in_toolbox(
                ["nm", "-D", binary_path],
                binary_path=binary_path,
            )
            if nm_res.exit_code == 0 and nm_res.stdout:
                for line in nm_res.stdout.split("\n"):
                    parts = line.strip().split()
                    if len(parts) >= 3 and parts[1] in ("U", "T", "W"):
                        if parts[1] == "U":
                            imports.append(parts[2])
                        else:
                            exports.append(parts[2])

        result["imports"] = imports
        result["exports"] = exports

        # ── linked libraries ────────────────────────────────────────
        ldd_res = self.run_in_toolbox(
            ["ldd", binary_path] if binary_path.startswith("/") else ["ldd", f"./{binary_path}"],
            binary_path=binary_path,
        )
        if ldd_res.exit_code == 0 and ldd_res.stdout:
            libraries = {}
            for line in ldd_res.stdout.split("\n"):
                line = line.strip()
                if "=>" in line and "(" in line:
                    name, rest = line.split("=>", 1)
                    path = rest.strip().split("(")[0].strip()
                    libraries[name.strip()] = path
            result["libraries"] = libraries
        else:
            result["libraries"] = {}

        result["success"] = True
        return result

    async def _arun(self, binary_path: str) -> dict[str, Any]:
        """Async variant — delegates to sync."""
        return self._run(binary_path)
