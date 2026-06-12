"""Function disassembly tool.

Disassembles a specific function in a binary using radare2 (with Ghidra
headless as a fallback for decompiled C pseudocode).
"""

from __future__ import annotations

import logging
from typing import Any, Optional

from pydantic import BaseModel, Field

from tools.base import BaseAnalysisTool

logger = logging.getLogger(__name__)


class DisassembleInput(BaseModel):
    """Input schema for function disassembly."""

    binary_path: str = Field(
        description="Path to the binary file in the sandbox"
    )
    function: str = Field(
        description="Function name or hex address (e.g., 'main' or '0x401000')"
    )
    use_ghidra: bool = Field(
        default=False,
        description="Use Ghidra headless decompiler (slower, provides C pseudocode)",
    )


class DisassembleFunctionTool(BaseAnalysisTool):
    """Disassemble a specific function and return assembly + pseudocode."""

    name: str = "disassemble_function"
    description: str = (
        "Disassemble a specific function in a binary. Returns assembly "
        "instructions via radare2, and optionally C pseudocode via Ghidra "
        "headless decompiler (use_ghidra=True)."
    )
    args_schema: type[BaseModel] = DisassembleInput

    def _run(
        self,
        binary_path: str,
        function: str,
        use_ghidra: bool = False,
    ) -> dict[str, Any]:
        """Execute function disassembly."""
        result: dict[str, Any] = {
            "binary": binary_path,
            "function": function,
            "success": False,
        }

        # ── radare2 disassembly ─────────────────────────────────────
        # Run r2 in pipe mode: analyze, seek to function, print disassembly
        r2_cmd = f"r2 -q -c 'aaa; afl~{function}; s {function}; pdf' {binary_path}"
        r2_res = self.run_in_toolbox(
            ["sh", "-c", r2_cmd],
            binary_path=binary_path,
            timeout=60,
        )

        if r2_res.error:
            result["error"] = r2_res.error
            return result

        if r2_res.exit_code != 0:
            result["r2_error"] = r2_res.stderr or f"Exit code: {r2_res.exit_code}"

        result["assembly"] = r2_res.stdout

        # ── Ghidra decompilation (optional) ─────────────────────────
        if use_ghidra:
            ghidra_result = self._decompile_with_ghidra(binary_path, function)
            if ghidra_result:
                result["pseudocode"] = ghidra_result
            else:
                result["pseudocode"] = None
                result["ghidra_note"] = "Ghidra decompilation not available or failed"

        result["success"] = True
        return result

    def _decompile_with_ghidra(
        self, binary_path: str, function: str
    ) -> Optional[str]:
        """Decompile a function using Ghidra headless mode."""
        import tempfile

        project_dir = "/tmp/ghidra_projects"
        project_name = "zerobot_proj"

        # Create a Ghidra script to decompile the function
        script = f"""
import ghidra.app.decompiler.DecompInterface;
import ghidra.util.task.TaskMonitor;

def decompile_function():
    if currentProgram is None:
        print("ERROR: No program loaded")
        return

    decomp = DecompInterface()
    decomp.openProgram(currentProgram)

    # Find the function
    func = getFunction("{function}")
    if func is None:
        print("ERROR: Function '{function}' not found")
        return

    # Decompile
    results = decomp.decompileFunction(func, 30, TaskMonitor.DUMMY)
    if results and results.decompiledFunction:
        print(results.decompiledFunction.getC())
    else:
        print("ERROR: Decompilation failed")

if __name__ == "__main__":
    decompile_function()
"""

        # Write script to temp file inside container
        script_path = "/tmp/decompile_script.py"
        ghidra_cmd = (
            f"echo {shlex.quote(script)} > {script_path} && "
            f"analyzeHeadless {project_dir} {project_name} "
            f"-import {binary_path} -scriptPath /tmp -postScript decompile_script.py"
        )

        res = self.run_in_toolbox(
            ["sh", "-c", ghidra_cmd],
            binary_path=binary_path,
            timeout=120,
        )

        if res.exit_code == 0 and res.stdout:
            return res.stdout
        return res.stderr or None

    async def _arun(
        self,
        binary_path: str,
        function: str,
        use_ghidra: bool = False,
    ) -> dict[str, Any]:
        """Async variant — delegates to sync."""
        return self._run(binary_path, function, use_ghidra)
