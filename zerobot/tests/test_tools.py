"""Unit tests for ZeroBot tools.

Mocks Docker calls to avoid requiring a running Docker daemon in tests.
"""

from __future__ import annotations

import json
from unittest.mock import MagicMock, PropertyMock, patch

import pytest

from tools.binary_analysis import BinaryAnalysisTool
from tools.crash_analyzer import CrashAnalyzerTool
from tools.disassemble import DisassembleFunctionTool
from tools.exploit_templates import ExploitTemplatesTool
from tools.fuzzing_harness import FuzzingHarnessTool
from tools.web_search import WebSearchTool
from sandbox.container_manager import ToolboxResult


# ── Fixtures ──────────────────────────────────────────────────────

@pytest.fixture
def mock_container_manager():
    """Create a mock ContainerManager that returns predefined results."""
    mock = MagicMock()
    mock.config = {"sandbox": {}}
    return mock


def make_result(
    stdout: str = "",
    stderr: str = "",
    exit_code: int = 0,
    error: str | None = None,
) -> ToolboxResult:
    return ToolboxResult(
        stdout=stdout,
        stderr=stderr,
        exit_code=exit_code,
        error=error,
    )


# ── Binary Analysis Tool ──────────────────────────────────────────

class TestBinaryAnalysisTool:
    def test_full_analysis(self, mock_container_manager):
        """Full analysis should return structured results."""
        tool = BinaryAnalysisTool(container_manager=mock_container_manager)

        # Mock sequential calls
        mock_container_manager.run_tool.side_effect = [
            make_result(stdout="ELF 64-bit LSB executable, x86-64"),
            make_result(stdout=json.dumps({
                "canary": True, "nx": True, "pie": True, "relro": "full"
            })),
            make_result(stdout="hello\nworld\nflag{}"),
            make_result(stdout="0x100 FUNC main\n0x200 IMPORT printf\n0x300 EXPORT vuln"),
            make_result(stdout="linux-vdso.so.1 => (0x7fff...)\nlibc.so.6 => /lib64/libc.so.6"),
        ]

        result = tool._run("/bin/ls")

        assert result["success"] is True
        assert "x86-64" in result["file_type"]
        assert result["protections"]["canary"] is True
        assert "printf" in result["imports"]
        assert "strings_sample" in result
        assert "libraries" in result
        assert mock_container_manager.run_tool.call_count == 5

    def test_analysis_error(self, mock_container_manager):
        """Analysis should handle Docker errors gracefully."""
        tool = BinaryAnalysisTool(container_manager=mock_container_manager)
        mock_container_manager.run_tool.return_value = make_result(
            error="Docker daemon not available"
        )

        result = tool._run("/bin/ls")
        assert result["success"] is False
        assert "error" in result


# ── Disassemble Tool ──────────────────────────────────────────────

class TestDisassembleTool:
    def test_disassemble_with_r2(self, mock_container_manager):
        """Disassembly should return assembly from radare2."""
        tool = DisassembleFunctionTool(container_manager=mock_container_manager)
        mock_container_manager.run_tool.return_value = make_result(
            stdout="0x401000  push rbp\n0x401001  mov rbp, rsp\n0x401004  sub rsp, 0x20",
            exit_code=0,
        )

        result = tool._run("/bin/test", "main")
        assert result["success"] is True
        assert "push rbp" in result["assembly"]

    def test_disassemble_function_not_found(self, mock_container_manager):
        """Missing function should still return r2 output (no crash)."""
        tool = DisassembleFunctionTool(container_manager=mock_container_manager)
        mock_container_manager.run_tool.return_value = make_result(
            stdout="",
            stderr="Cannot find function 'nonexistent'",
            exit_code=1,
        )

        result = tool._run("/bin/test", "nonexistent")
        assert result["success"] is True  # Tool doesn't error, returns r2 output
        assert result.get("r2_error")


# ── Fuzzing Harness Tool ──────────────────────────────────────────

class TestFuzzingHarnessTool:
    def test_generate_harness(self, mock_container_manager):
        """Harness generation should produce valid C code."""
        tool = FuzzingHarnessTool(container_manager=mock_container_manager)

        result = tool._run(
            target_name="parse_packet",
            function_declaration="int parse_packet(const char *data, size_t len)",
            setup_description="The function takes a buffer from fuzz input",
        )

        assert result["success"] is True
        assert "LLVMFuzzerTestOneInput" in result["harness_c"]
        assert "parse_packet" in result["harness_c"]
        assert result["function"] == "parse_packet"

    def test_compile_instructions_present(self, mock_container_manager):
        """Result should include compilation instructions."""
        tool = FuzzingHarnessTool(container_manager=mock_container_manager)

        result = tool._run(
            target_name="test_func",
            function_declaration="void test_func(char *input)",
        )

        assert "compile_instructions" in result
        assert "libfuzzer" in result["compile_instructions"]
        assert "afl" in result["compile_instructions"]

    def test_string_harness(self, mock_container_manager):
        """String-based setup should generate appropriate code."""
        tool = FuzzingHarnessTool(container_manager=mock_container_manager)

        result = tool._run(
            target_name="parse_input",
            function_declaration="void parse_input(char *input)",
            setup_description="string input expected",
        )

        assert "input_str" in result["harness_c"]


# ── Crash Analyzer Tool ───────────────────────────────────────────

class TestCrashAnalyzerTool:
    def test_core_dump_analysis(self, mock_container_manager):
        """Core dump analysis should parse GDB output."""
        tool = CrashAnalyzerTool(container_manager=mock_container_manager)

        mock_container_manager.run_tool.return_value = make_result(
            stdout="""#0  0x0000000000401234 in vuln_function ()
#1  0x4141414141414141 in ?? ()
rax            0x41414141
rbx            0x0
rcx            0x41414141
rdx            0x0
rip            0x4141414141414141
rsp            0x7fffffffe000
0x7fffffffe000: 0x41414141 0x41414141 0x41414141 0x41414141
""",
            exit_code=0,
        )

        result = tool._run(binary_path="/bin/test", core_path="/tmp/core")

        assert result["success"] is True
        assert "rip/eip control" in result["crash_type"].lower()
        assert "exploitable" in result["exploitability"].lower()

    def test_report_only_analysis(self, mock_container_manager):
        """Text crash report should still return diagnosis."""
        tool = CrashAnalyzerTool(container_manager=mock_container_manager)

        result = tool._run(
            binary_path="/bin/test",
            crash_report="SIGSEGV at address 0x41414141",
        )

        assert result["success"] is True
        assert "segmentation fault" in result["crash_type"].lower()

    def test_no_input(self, mock_container_manager):
        """No core or report should return an error."""
        tool = CrashAnalyzerTool(container_manager=mock_container_manager)

        result = tool._run(binary_path="/bin/test")
        assert result["success"] is False


# ── Exploit Templates Tool ────────────────────────────────────────

class TestExploitTemplatesTool:
    @pytest.mark.parametrize("vuln_type", [
        "buffer_overflow", "heap_uaf", "format_string",
    ])
    def test_known_templates(self, mock_container_manager, vuln_type):
        """Known vulnerability types should return templates."""
        tool = ExploitTemplatesTool(container_manager=mock_container_manager)

        result = tool._run(vulnerability_type=vuln_type)
        assert result["success"] is True
        assert result["educational"] is True

    def test_rop_chain_template(self, mock_container_manager):
        """ROP chain template should return C code."""
        tool = ExploitTemplatesTool(container_manager=mock_container_manager)

        result = tool._run(vulnerability_type="rop_chain")
        assert result["success"] is True
        assert "ROP" in result["source_code"]

    def test_shellcode_template(self, mock_container_manager):
        """Shellcode template should return placeholder."""
        tool = ExploitTemplatesTool(container_manager=mock_container_manager)

        result = tool._run(vulnerability_type="shellcode")
        assert result["success"] is True
        assert "placeholder" in result["source_code"]

    def test_unknown_template(self, mock_container_manager):
        """Unknown type should return an error."""
        tool = ExploitTemplatesTool(container_manager=mock_container_manager)

        result = tool._run(vulnerability_type="unknown_type_xyz")
        assert result["success"] is False

    def test_warning_present(self, mock_container_manager):
        """All templates should include an educational warning."""
        tool = ExploitTemplatesTool(container_manager=mock_container_manager)

        result = tool._run(vulnerability_type="buffer_overflow")
        assert "warning" in result
        assert "EDUCATIONAL" in result["warning"].upper()


# ── Web Search Tool ───────────────────────────────────────────────

class TestWebSearchTool:
    def test_no_vectorstore(self):
        """Without a vector store, should return helpful error."""
        tool = WebSearchTool(
            chromadb_path="/nonexistent/path",
        )

        result = tool._run("test query")
        assert result["success"] is False
        assert "error" in result

    def test_search_query_format(self, mock_container_manager):
        """Search should accept standard query strings."""
        tool = WebSearchTool(
            container_manager=mock_container_manager,
            chromadb_path="/nonexistent/path",
        )

        result = tool._run("buffer overflow in libpng", max_results=3)
        # Should fail gracefully due to missing DB
        assert result["success"] is False
        assert "CVE database not available" in result["error"]
