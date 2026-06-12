"""ZeroBot CLI entry point.

Allows running ZeroBot as a command-line tool:
    python -m zerobot
"""

import argparse
import logging
import sys
import textwrap

import yaml

from agent.orchestrator import ZeroBotAgent
from ethics.filters import EthicalFilter
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


def create_cli_agent() -> ZeroBotAgent:
    """Create a ZeroBot agent for CLI usage with Ollama."""
    from langchain_ollama import ChatOllama

    llm = ChatOllama(
        base_url="http://localhost:11434",
        model="mistral:7b-instruct",
        temperature=0.1,
    )

    tools = [
        BinaryAnalysisTool(),
        DisassembleFunctionTool(),
        FuzzingHarnessTool(),
        CrashAnalyzerTool(),
        ExploitTemplatesTool(),
        WebSearchTool(),
    ]

    ethical_filter = EthicalFilter(allowed_hashes=["*"])

    return ZeroBotAgent(
        llm=llm,
        tools=tools,
        retriever=None,
        ethical_filter=ethical_filter,
    )


def interactive_mode(agent: ZeroBotAgent):
    """Run ZeroBot in interactive CLI mode."""
    print(textwrap.dedent("""\
        ╔═══════════════════════════════════════════╗
        ║  ZeroBot — Vulnerability Research Assistant  ║
        ║  Type 'exit' to quit, '/help' for help      ║
        ╚═══════════════════════════════════════════╝
    """))

    while True:
        try:
            user_input = input("\n> ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nGoodbye.")
            break

        if not user_input:
            continue
        if user_input.lower() in ("exit", "quit"):
            print("Goodbye.")
            break
        if user_input == "/help":
            print("Commands: exit/quit (exit), /help (this help)")
            print("Tools available: binary_analysis, disassemble_function,")
            print("  fuzzing_harness, crash_analyzer, exploit_templates, web_search")
            continue

        result = agent.process_message(user_input, session_id="cli")
        response = result.get("response", "No response.")

        if result.get("blocked"):
            print(f"\n[BLOCKED] {response}")
        else:
            print(f"\n{response}")


def main():
    parser = argparse.ArgumentParser(
        description="ZeroBot — Vulnerability Research Assistant"
    )
    parser.add_argument(
        "--cli", action="store_true",
        help="Run in interactive CLI mode"
    )
    parser.add_argument(
        "--message", "-m", type=str,
        help="Single message to process (non-interactive)"
    )

    args = parser.parse_args()

    if args.cli:
        agent = create_cli_agent()
        interactive_mode(agent)
    elif args.message:
        agent = create_cli_agent()
        result = agent.process_message(args.message)
        print(result.get("response", ""))
    else:
        print("Use --cli for interactive mode or --message for single query.")


if __name__ == "__main__":
    main()
