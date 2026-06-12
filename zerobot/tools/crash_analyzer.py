"""Crash dump analysis tool.

Takes a core dump (or fuzzer crash report) and the associated binary,
and produces a structured diagnostic: faulting instruction, registers,
backtrace, and exploitability assessment.
"""

from __future__ import annotations

import logging
import re
from typing import Any, Optional

from pydantic import BaseModel, Field

from tools.base import BaseAnalysisTool

logger = logging.getLogger(__name__)


class CrashAnalyzerInput(BaseModel):
    """Input schema for crash analysis."""

    binary_path: str = Field(
        description="Path to the binary that crashed (in the sandbox)"
    )
    core_path: Optional[str] = Field(
        default=None,
        description="Path to the core dump file (in the sandbox)",
    )
    crash_report: Optional[str] = Field(
        default=None,
        description="Text crash report from AFL/libFuzzer (if no core dump)",
    )


class CrashAnalyzerTool(BaseAnalysisTool):
    """Analyze a crash dump and produce a diagnostic report."""

    name: str = "crash_analyzer"
    description: str = (
        "Analyze a core dump or fuzzer crash report together with the "
        "associated binary. Returns: crash type, faulting instruction, "
        "register state, backtrace, stack layout, and exploitability assessment."
    )
    args_schema: type[BaseModel] = CrashAnalyzerInput

    def _run(
        self,
        binary_path: str,
        core_path: Optional[str] = None,
        crash_report: Optional[str] = None,
    ) -> dict[str, Any]:
        """Execute crash analysis."""
        result: dict[str, Any] = {
            "binary": binary_path,
            "crash_type": "unknown",
            "exploitability": "unknown",
            "success": False,
        }

        if core_path:
            # ── GDB analysis of core dump ───────────────────────────
            result.update(self._analyze_core(binary_path, core_path))
        elif crash_report:
            # ── Text-based crash report analysis ────────────────────
            result.update(self._analyze_report(crash_report))
        else:
            result["error"] = "No core dump or crash report provided."

        result["success"] = "error" not in result
        return result

    def _analyze_core(self, binary_path: str, core_path: str) -> dict[str, Any]:
        """Use GDB batch mode to analyze a core dump."""
        result: dict[str, Any] = {}

        # GDB batch commands
        gdb_commands = [
            f"core-file {core_path}",
            "bt",
            "info registers",
            "x/16xw $sp",
            "info frame",
            "bt full",
            "quit",
        ]
        gdb_script = "\n".join(gdb_commands)
        gdb_cmd = f"gdb -batch -ex '{gdb_commands[0]}' -ex 'bt' -ex 'info registers' -ex 'x/16xw \\$sp' -ex 'info frame' -ex 'bt full' {binary_path}"

        res = self.run_in_toolbox(
            ["sh", "-c", gdb_cmd],
            binary_path=binary_path,
            timeout=60,
        )

        if res.error:
            result["gdb_error"] = res.error
            return result
        if res.exit_code != 0:
            result["gdb_stderr"] = res.stderr

        gdb_output = res.stdout

        # Parse backtrace
        backtrace = []
        in_bt = False
        for line in gdb_output.split("\n"):
            if line.startswith("#"):
                backtrace.append(line.strip())
            elif "Backtrace" in line and "---" not in line:
                in_bt = True
            elif in_bt and line.strip() and not line.startswith(" " * 4):
                backtrace.append(line.strip())

        result["backtrace"] = backtrace

        # Parse registers
        registers = {}
        reg_section = False
        for line in gdb_output.split("\n"):
            if re.match(r"^[er][a-z]{2}\s+0x", line) or re.match(r"^r[0-9]+", line):
                parts = line.split()
                if len(parts) >= 2:
                    reg_name = parts[0]
                    reg_value = parts[1]
                    registers[reg_name] = reg_value
            elif "Register dump" in line:
                reg_section = True

        result["registers"] = registers

        # Check for controlled registers (exploitability signal)
        controlled_regs = []
        for reg, val in registers.items():
            if val in ("0x41414141", "0x42424242", "0x61616161"):
                controlled_regs.append(reg)

        # Parse stack
        stack = []
        for line in gdb_output.split("\n"):
            if line.strip().startswith("0x") and "/" in line or ":" in line:
                stack.append(line.strip())

        result["stack_sample"] = stack[:16]
        result["controlled_registers"] = controlled_regs

        # Exploitability assessment
        result["crash_type"] = self._classify_crash(backtrace, registers, gdb_output)
        result["exploitability"] = self._assess_exploitability(
            backtrace, registers, controlled_regs, gdb_output
        )

        return result

    def _analyze_report(self, report: str) -> dict[str, Any]:
        """Analyze a text crash report from AFL/libFuzzer."""
        result: dict[str, Any] = {}

        # Common AFL report patterns
        afl_patterns = {
            "crash_signal": re.compile(r"(?i)(signal|SIGSEGV|SIGABRT|SIGFPE|SIGILL)\s*[=:]\s*(\d+)"),
            "fault_addr": re.compile(r"(?i)(fault.addr|crash.address|PC)\s*[=:]\s*(0x[0-9a-f]+)"),
            "map_size": re.compile(r"(?i)map.size\s*[=:]\s*(\d+)"),
            "exec_speed": re.compile(r"(?i)exec.speed\s*[=:]\s*(\d+)"),
        }

        extracted = {}
        for key, pattern in afl_patterns.items():
            m = pattern.search(report)
            if m:
                extracted[key] = m.group(0)

        result["crash_report_parsed"] = extracted
        result["crash_type"] = self._classify_from_report(report)
        result["exploitability"] = "unknown — core dump required for deeper diagnosis"

        # Suggest GDB analysis
        result["recommendation"] = (
            "For a complete diagnosis, provide the binary and core dump "
            "so GDB can produce a full backtrace and register state."
        )

        return result

    def _classify_crash(
        self,
        backtrace: list[str],
        registers: dict[str, str],
        gdb_output: str,
    ) -> str:
        """Classify the type of crash."""
        crash_type = "unknown"

        # Check for common crash signatures
        if "stack overflow" in gdb_output.lower():
            crash_type = "stack overflow"
        elif "heap overflow" in gdb_output.lower():
            crash_type = "heap overflow"
        elif "double free" in gdb_output.lower() or "free(): double free" in gdb_output:
            crash_type = "double free"
        elif "use after free" in gdb_output.lower():
            crash_type = "use after free"
        elif "null" in gdb_output.lower() and (
            "dereference" in gdb_output.lower() or "pointer" in gdb_output.lower()
        ):
            crash_type = "null pointer dereference"
        elif any("0x41414141" in v for v in registers.values()):
            crash_type = "rip/eip control (buffer overflow)"
        elif any("0x42424242" in v for v in registers.values()):
            crash_type = "potential buffer overflow"
        elif "SIGSEGV" in gdb_output:
            crash_type = "segmentation fault"

        return crash_type

    def _assess_exploitability(
        self,
        backtrace: list[str],
        registers: dict[str, str],
        controlled_regs: list[str],
        gdb_output: str,
    ) -> str:
        """Assess whether the crash appears exploitable."""
        if not backtrace and not controlled_regs:
            return "unknown — insufficient data"

        signals = []

        # RIP/EIP control
        rip = registers.get("rip") or registers.get("eip")
        if rip and "41414141" in rip:
            signals.append("RIP/EIP fully controlled by user data")
        elif rip and ("42424242" in rip or "61616161" in rip):
            signals.append("RIP/EIP partially controlled")

        # Stack pointer control
        rsp = registers.get("rsp") or registers.get("esp")
        if rsp and "41414141" in rsp:
            signals.append("Stack pointer controlled")

        # Other controlled registers
        if controlled_regs:
            signals.append(
                f"Controlled registers: {', '.join(controlled_regs)}"
            )

        if not signals:
            return "likely not exploitable — no controlled registers"

        result = "potentially exploitable — " + "; ".join(signals)
        return result

    def _classify_from_report(self, report: str) -> str:
        """Classify crash type from a text report."""
        if "SIGSEGV" in report:
            return "segmentation fault"
        elif "SIGABRT" in report:
            return "abort (assertion failure or heap corruption likely)"
        elif "SIGFPE" in report:
            return "arithmetic exception (divide by zero)"
        elif "SIGILL" in report:
            return "illegal instruction"
        return "unknown"

    async def _arun(
        self,
        binary_path: str,
        core_path: Optional[str] = None,
        crash_report: Optional[str] = None,
    ) -> dict[str, Any]:
        """Async variant — delegates to sync."""
        return self._run(binary_path, core_path, crash_report)
