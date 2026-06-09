#!/bin/bash
# SoulSystem Autonomous Entity Launcher
# Usage: ./launch-autonomous.sh [mode]
# Modes: repl, ask, plan, status

set -e

BINARY="./target/release/soulsystem"
MODEL="qwen3:4b"

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo "Building release binary..."
    cargo build --release -p soulsystem
fi

# Check if Ollama is running
if ! curl -s http://127.0.0.1:11434/api/tags > /dev/null 2>&1; then
    echo "Error: Ollama is not running on 127.0.0.1:11434"
    echo "Start it with: ollama serve"
    exit 1
fi

# Check if model exists
if ! curl -s http://127.0.0.1:11434/api/tags | grep -q "$MODEL"; then
    echo "Model $MODEL not found. Pulling..."
    ollama pull $MODEL
fi

MODE=${1:-repl}

case $MODE in
    repl)
        echo "Starting Autonomous REPL..."
        $BINARY --repl
        ;;
    ask)
        if [ -z "$2" ]; then
            echo "Usage: $0 ask \"your question\""
            exit 1
        fi
        $BINARY --ask "$2"
        ;;
    plan)
        if [ -z "$2" ]; then
            echo "Usage: $0 plan \"goal description\""
            exit 1
        fi
        $BINARY --plan "$2"
        ;;
    status)
        echo "=== SoulSystem Autonomous Status ==="
        echo ""
        echo "Ollama:"
        curl -s http://127.0.0.1:11434/api/tags | python3 -c "
import sys, json
data = json.load(sys.stdin)
print(f'  Models: {len(data[\"models\"])}')
for m in data['models'][:5]:
    print(f'    - {m[\"name\"]}')
"
        echo ""
        echo "Binary: $BINARY ($(ls -lh $BINARY | awk '{print $5}'))"
        echo ""
        echo "Run '$0 repl' to start interactive mode"
        ;;
    *)
        echo "Usage: $0 [repl|ask|plan|status]"
        echo ""
        echo "Modes:"
        echo "  repl           - Interactive REPL (default)"
        echo "  ask \"question\" - Ask a question"
        echo "  plan \"goal\"    - Create a plan"
        echo "  status         - Show system status"
        exit 1
        ;;
esac
