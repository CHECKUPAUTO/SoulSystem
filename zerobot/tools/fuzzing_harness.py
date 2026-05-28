"""Fuzzing harness generator tool.

Generates a C source file and CMakeLists.txt for libFuzzer/AFL++ harness
from a function signature description. Does NOT compile or run the harness.
"""

from __future__ import annotations

import logging
from typing import Any

from pydantic import BaseModel, Field

from tools.base import BaseAnalysisTool

logger = logging.getLogger(__name__)

HARNESS_TEMPLATE = """\
// ZeroBot — Auto-generated Fuzzing Harness
// Target: {target}
// Compile with libFuzzer or AFL++
// IMPORTANT: Only use on authorized targets

#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

// Forward declaration of the target function
// User must provide the actual declaration matching the target binary
{function_declaration}

// libFuzzer entry point
int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {{
    // --- User: customize input parsing below ---

    // Example: pass raw data to a buffer
    // The target function's actual parameters should be derived
    // from the fuzz input here.

    {setup_code}

    // Call the target function
    // {target_call}

    return 0;  // Non-zero return values are reserved (see libFuzzer docs)
}}
"""

CMAKE_TEMPLATE = """\
cmake_minimum_required(VERSION 3.16)
project(fuzz_harness)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

# libFuzzer mode (Clang only)
# Compile: mkdir build && cd build && CXX=clang++ cmake .. -DFUZZER=libfuzzer && make
option(FUZZER "Fuzzer backend" "standalone")

if(FUZZER STREQUAL "libfuzzer")
    set(CMAKE_CXX_FLAGS "${{CMAKE_CXX_FLAGS}} -fsanitize=fuzzer,address,undefined")
    set(CMAKE_C_FLAGS "${{CMAKE_C_FLAGS}} -fsanitize=fuzzer,address,undefined")
elseif(FUZZER STREQUAL "afl")
    set(CMAKE_CXX_FLAGS "${{CMAKE_CXX_FLAGS}} -fsanitize=address,undefined")
    set(CMAKE_C_FLAGS "${{CMAKE_C_FLAGS}} -fsanitize=address,undefined")
    # AFL++ requires afl-clang-fast compiler wrapper
else()
    # Standalone mode: provide a main() that reads stdin
    add_definitions(-DSTANDALONE)
endif()

add_executable(fuzz_harness
    harness.c
)
"""


class FuzzingHarnessInput(BaseModel):
    """Input schema for fuzzing harness generation."""

    target_name: str = Field(
        description="Name of the target function (e.g., 'parse_packet')"
    )
    function_declaration: str = Field(
        description="C function declaration of the target "
        "(e.g., 'int parse_packet(const char *data, size_t len)')"
    )
    setup_description: str = Field(
        default="",
        description="Description of how the fuzz input should be mapped to function parameters",
    )


class FuzzingHarnessTool(BaseAnalysisTool):
    """Generate a fuzzing harness C source file and build configuration."""

    name: str = "fuzzing_harness"
    description: str = (
        "Generate a C source file and CMakeLists.txt for libFuzzer/AFL++ "
        "fuzzing harness. Takes a function declaration and produces compilable "
        "skeleton code. Does NOT compile or execute the harness."
    )
    args_schema: type[BaseModel] = FuzzingHarnessInput

    def _run(
        self,
        target_name: str,
        function_declaration: str,
        setup_description: str = "",
    ) -> dict[str, Any]:
        """Generate fuzzing harness files."""
        # Parse function name from declaration
        func_name = target_name.split("(")[0].strip().split()[-1] if "(" in target_name else target_name

        # Generate setup code based on description
        setup_lines = []
        target_args = "data, size"

        if "file" in setup_description.lower() or "buffer" in setup_description.lower():
            setup_lines.append("// Fuzz input maps directly to a buffer")
            setup_lines.append(f"    const uint8_t *input = data;")
            setup_lines.append(f"    size_t input_size = size;")
            target_args = "input, input_size"

        if "string" in setup_description.lower():
            setup_lines.append("// Treat input as a null-terminated string")
            setup_lines.append(
                f"    char input_str[1024];"
            )
            setup_lines.append(
                f"    size_t copy_size = size < sizeof(input_str) - 1 ? size : sizeof(input_str) - 1;"
            )
            setup_lines.append(f"    memcpy(input_str, data, copy_size);")
            setup_lines.append(f"    input_str[copy_size] = '\\0';")
            target_args = "input_str"

        if not setup_lines:
            setup_lines.append(
                f"    // Pass raw fuzz data directly to the target"
            )
            setup_lines.append(f"    // Modify this section to match the target function signature")

        setup_code = "\n    ".join(setup_lines)
        target_call = f"    {func_name}({target_args});"

        # Render templates
        harness_source = HARNESS_TEMPLATE.format(
            target=target_name,
            function_declaration=function_declaration,
            setup_code=setup_code,
            target_call=target_call,
        )

        cmake_source = CMAKE_TEMPLATE

        # Compilation instructions
        compile_instructions = {
            "libfuzzer": f"clang -fsanitize=fuzzer,address,undefined -o fuzz_harness harness.c",
            "afl": (
                "AFL_USE_ASAN=1 afl-clang-fast -o fuzz_harness harness.c\n"
                "  # Then: afl-fuzz -i input_dir -o output_dir ./fuzz_harness"
            ),
            "standalone": "gcc -o fuzz_harness harness.c -DSTANDALONE  # reads from stdin",
        }

        return {
            "success": True,
            "target": target_name,
            "function": func_name,
            "harness_c": harness_source,
            "cmakelists_txt": cmake_source,
            "compile_instructions": compile_instructions,
            "warning": (
                "This harness is a skeleton — you must adapt the input parsing "
                "to match the target function's exact signature. "
                "Run the compiled harness only on authorized targets."
            ),
        }

    async def _arun(
        self,
        target_name: str,
        function_declaration: str,
        setup_description: str = "",
    ) -> dict[str, Any]:
        """Async variant — delegates to sync."""
        return self._run(target_name, function_declaration, setup_description)
