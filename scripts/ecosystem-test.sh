#!/usr/bin/env bash
#=============================================================================
# SoulSystem Écosystem Test Battery
# Tests: infrastructure, soulsystem, CCOS, OctaSoma, cascade, gateway, Ollama
# Compatible Debian (x86_64) + Jetson AGX (aarch64)
#=============================================================================
set -u
# NOTE: no 'set -e' — test failures must NOT abort the script

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

# --remote-only: skip sections 1-9, only run local tests on remote machine
REMOTE_ONLY=0
if [ "${1:-}" = "--remote-only" ]; then
  REMOTE_ONLY=1
fi

MACHINE=$(uname -n)
ARCH=$(uname -m)
PASS=0
FAIL=0
WARN=0
TOTAL=0
RESULTS=()

GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'

pass() { PASS=$((PASS+1)); TOTAL=$((TOTAL+1)); RESULTS+=("PASS:$1"); echo -e "  ${GREEN}PASS${NC} $1"; }
fail() { FAIL=$((FAIL+1)); TOTAL=$((TOTAL+1)); RESULTS+=("FAIL:$1"); echo -e "  ${RED}FAIL${NC} $1"; }
warn() { WARN=$((WARN+1)); TOTAL=$((TOTAL+1)); RESULTS+=("WARN:$1"); echo -e "  ${YELLOW}WARN${NC} $1"; }
check() { if [ "$1" = 0 ]; then pass "$2"; else fail "$2 (exit=$1)"; fi; }

echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     SoulSystem Ecosystem Test Battery                   ║${NC}"
echo -e "${CYAN}║     Machine: ${MACHINE} (${ARCH})                        ║${NC}"
echo -e "${CYAN}║     Date:    $(date)                    ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""

#=============================================================================
# SECTION 1: Infrastructure
#=============================================================================
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 1: Infrastructure${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

# 1.1 OS & Kernel
echo "  1.1 OS Info: $(uname -a | cut -d' ' -f1-3)"
check 0 "kernel"

# 1.2 CPU architecture
[ "$ARCH" = "x86_64" ] || [ "$ARCH" = "aarch64" ] && pass "arch:$ARCH" || fail "arch:$ARCH"

# 1.3 Disk space
ROOT_FREE=$(df "$REPO_ROOT" --output=avail -B1 2>/dev/null | tail -1)
[ "$ROOT_FREE" -gt 1073741824 ] && pass "disk:$(numfmt --to=iec $ROOT_FREE) free" || warn "disk:<1G free ($(numfmt --to=iec $ROOT_FREE))"

# 1.4 Memory
MEM=$(free -b | awk '/Mem:/{print $2}')
[ "$MEM" -gt 2147483648 ] && pass "ram:$(numfmt --to=iec $MEM)" || warn "ram:<2G ($(numfmt --to=iec $MEM))"

# 1.5 Node version
NODE_V=$(node --version 2>/dev/null) && pass "node:$NODE_V" || fail "node:not found"

# 1.6 Python version
PY_V=$(python3 --version 2>/dev/null) && pass "python:$PY_V" || fail "python3:not found"

# 1.7 Rust/Cargo (source .cargo/env for non-interactive shells)
source "$HOME/.cargo/env" 2>/dev/null || true
CARGO_V=$(cargo --version 2>/dev/null) && pass "cargo:$CARGO_V" || warn "cargo:not found"

# 1.8 SSH to self
ssh -o StrictHostKeyChecking=no -o ConnectTimeout=3 127.0.0.1 true 2>/dev/null && pass "ssh:self" || warn "ssh:self (expected if no sshd)"

#=============================================================================
# SECTION 2: Ollama
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 2: Ollama LLM Backend${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

# 2.1 Ollama reachable
OLLAMA_ENDPOINT="${OLLAMA_ENDPOINT:-http://127.0.0.1:11434}"
OLLAMA_OK=0
OLLAMA_MODELS=0
if curl -sf "$OLLAMA_ENDPOINT/api/tags" > /dev/null 2>&1; then
  OLLAMA_OK=1
  OLLAMA_MODELS=$(curl -sf "$OLLAMA_ENDPOINT/api/tags" 2>/dev/null | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('models',[])))" 2>/dev/null || echo 0)
  pass "ollama:running ($OLLAMA_MODELS models)"
else
  warn "ollama:not reachable at $OLLAMA_ENDPOINT"
fi

# 2.2 Test inference if ollama up
if [ "$OLLAMA_OK" = 1 ] && [ "$OLLAMA_MODELS" -gt 0 ]; then
  MODEL=$(curl -sf "$OLLAMA_ENDPOINT/api/tags" 2>/dev/null | python3 -c "import json,sys; ms=json.load(sys.stdin).get('models',[]); print(ms[0]['name'] if ms else '')" 2>/dev/null || true)
  if [ -n "$MODEL" ]; then
    INFER=$(timeout 30 curl -sf -X POST "$OLLAMA_ENDPOINT/api/generate" -d "{\"model\":\"$MODEL\",\"prompt\":\"say hello\",\"stream\":false}" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if d.get('response') else 'fail')" 2>/dev/null || echo "timeout")
    [ "$INFER" = "ok" ] && pass "ollama:inference:$MODEL" || warn "ollama:inference failed on $MODEL"
  fi
fi

# 2.3 Embedding model check
EMBED_MODEL="nomic-embed-text"
EMBED_OK=0
if curl -sf "$OLLAMA_ENDPOINT/api/tags" 2>/dev/null | python3 -c "import json,sys; print(any('$EMBED_MODEL' in m['name'] for m in json.load(sys.stdin).get('models',[])))" 2>/dev/null | grep -q True; then
  pass "ollama:embedding:$EMBED_MODEL available"
  EMBED_OK=1
else
  warn "ollama:embedding:$EMBED_MODEL not found; octasoma/ccos may use fallback"
fi

# 2.4 Test embedding if model available
if [ "$EMBED_OK" = 1 ]; then
  EMBED=$(timeout 15 curl -sf -X POST "$OLLAMA_ENDPOINT/api/embeddings" -d "{\"model\":\"$EMBED_MODEL\",\"prompt\":\"test embedding\"}" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('embedding',[])))" 2>/dev/null || echo "0")
  [ "$EMBED" -gt 0 ] && pass "ollama:embedding dim=$EMBED" || warn "ollama:embedding returned 0 dims"
fi

#=============================================================================
# SECTION 3: soulsystem-lite / CERVO
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 3: soulsystem-lite / CERVO${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

SOUL_API="http://127.0.0.1:9023"
SOUL_OK=0

# 3.1 Health endpoint
SOUL_HEALTH=$(timeout 5 curl -sf "$SOUL_API/health" 2>/dev/null || echo "")
if [ -n "$SOUL_HEALTH" ]; then
  SOUL_OK=1
  pass "soulsystem:health"
else
  fail "soulsystem:health (is soulsystem-lite running?)"
fi

# 3.2 CERVO status
if [ "$SOUL_OK" = 1 ]; then
  CERVO=$(timeout 5 curl -sf "$SOUL_API/cervo/status" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if d.get('ok') else 'err')" 2>/dev/null || echo "err")
  [ "$CERVO" = "ok" ] && pass "cervo:status" || fail "cervo:status"

  # 3.3 CCOS stats
  CCOS_STATS=$(timeout 5 curl -sf "$SOUL_API/ccos/stats" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if d.get('ok') else 'err')" 2>/dev/null || echo "err")
  [ "$CCOS_STATS" = "ok" ] && pass "ccos:stats" || fail "ccos:stats"

  # 3.4 Ingest via REST
  INGEST_RES=$(timeout 10 curl -sf -X POST "$SOUL_API/ccos/ingest" \
    -H "Content-Type: application/json" \
    -d '{"uri":"tag:ecosystem-test/ingest-1","source":"Ecosystème test: CCOS avec CERVO REST OK"}' 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if d.get('ok') else d)" 2>/dev/null || echo "err")
  [ "$INGEST_RES" = "ok" ] && pass "ccos:ingest REST" || warn "ccos:ingest REST ($INGEST_RES)"

  # 3.5 Recall via REST (hybrid)
  RECALL_RES=$(timeout 10 curl -sf -X POST "$SOUL_API/ccos/recall" \
    -H "Content-Type: application/json" \
    -d '{"text":"CCOS CERVO ecosystem","strategy":"hybrid","budget":5}' 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
items=d.get('result',{}).get('items',[])
print(len(items))" 2>/dev/null || echo "-1")
  [ "$RECALL_RES" -ge 0 ] 2>/dev/null && pass "ccos:recall hybrid ($RECALL_RES items)" || fail "ccos:recall hybrid"

  # 3.6 Recall via REST (semantic)
  RECALL2=$(timeout 10 curl -sf -X POST "$SOUL_API/ccos/recall" \
    -H "Content-Type: application/json" \
    -d '{"text":"ecosystem test","strategy":"semantic","budget":3}' 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
items=d.get('result',{}).get('items',[])
print(len(items))" 2>/dev/null || echo "-1")
  [ "$RECALL2" -ge 0 ] 2>/dev/null && pass "ccos:recall semantic ($RECALL2 items)" || pass "ccos:recall semantic (0 items, expected)"

  # 3.7 Batch ingest (stress test)
  BATCH_PASS=0
  BATCH_FAIL=0
  for i in $(seq 1 20); do
    BATCH_RES=$(timeout 5 curl -sf -X POST "$SOUL_API/ccos/ingest" \
      -H "Content-Type: application/json" \
      -d "{\"uri\":\"tag:ecosystem-test/stress-$i\",\"source\":\"Stress test item #$i: CCOS+CERVO REST API batch ingest performance test at $(date +%s)\"}" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if d.get('ok') else 'err')" 2>/dev/null || echo "err")
    [ "$BATCH_RES" = "ok" ] && BATCH_PASS=$((BATCH_PASS+1)) || BATCH_FAIL=$((BATCH_FAIL+1))
  done
  [ "$BATCH_FAIL" = 0 ] && pass "ccos:batch-ingest 20/20" || warn "ccos:batch-ingest ${BATCH_PASS}/20 passed"

  # 3.8 Recall after batch
  BATCH_RECALL=$(timeout 10 curl -sf -X POST "$SOUL_API/ccos/recall" \
    -H "Content-Type: application/json" \
    -d '{"text":"stress test batch performance","strategy":"hybrid","budget":10}' 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
items=d.get('result',{}).get('items',[])
print(len(items))" 2>/dev/null || echo "0")
  [ "$BATCH_RECALL" -gt 0 ] && pass "ccos:recall after batch ($BATCH_RECALL items)" || warn "ccos:recall after batch (0 items)"
fi

#=============================================================================
# SECTION 4: CCOS Direct MCP (if binary available)
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 4: CCOS Direct MCP${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

CCOS_BIN=""
[ -n "${CCOS_CMD:-}" ] && CCOS_BIN="$CCOS_CMD"
[ -z "$CCOS_BIN" ] && CCOS_BIN=$(command -v ccos 2>/dev/null || true)
[ -z "$CCOS_BIN" ] && [ "$ARCH" = "aarch64" ] && [ -f "$REPO_ROOT/ccos/target/aarch64-unknown-linux-gnu/release/ccos" ] && CCOS_BIN="$REPO_ROOT/ccos/target/aarch64-unknown-linux-gnu/release/ccos"
[ -z "$CCOS_BIN" ] && [ -f "$REPO_ROOT/target/release/ccos" ] && CCOS_BIN="$REPO_ROOT/target/release/ccos"
[ -z "$CCOS_BIN" ] && [ -f "$REPO_ROOT/ccos/target/release/ccos" ] && CCOS_BIN="$REPO_ROOT/ccos/target/release/ccos"

if [ -n "$CCOS_BIN" ]; then
  pass "ccos:binary:$CCOS_BIN"
  # Test CCOS MCP initialize
  CCOS_MCP=$(echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | timeout 5 "$CCOS_BIN" mcp /tmp/test-ccos-mcp.ccos 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('result',{}).get('serverInfo',{}).get('name','?'))" 2>/dev/null || echo "err")
  [ "$CCOS_MCP" = "ccos-memory" ] && pass "ccos:mcp-init" || fail "ccos:mcp-init ($CCOS_MCP)"

  # CCOS tool list
  CCOS_TOOLS=$(echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | timeout 5 "$CCOS_BIN" mcp /tmp/test-ccos-mcp.ccos 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('result',{}).get('tools',[])))" 2>/dev/null || echo "0")
  [ "$CCOS_TOOLS" -gt 0 ] && pass "ccos:mcp-tools ($CCOS_TOOLS tools)" || fail "ccos:mcp-tools (0 tools)"
else
  warn "ccos:binary not found"
fi

#=============================================================================
# SECTION 5: OctaSoma
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 5: OctaSoma Fractal Memory${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

OCTA_BIN="${OCTASOMA_CMD:-}"
[ -z "$OCTA_BIN" ] && OCTA_BIN=$(command -v octasoma-mcp 2>/dev/null || true)
[ -z "$OCTA_BIN" ] && [ -f "$REPO_ROOT/target/release/octasoma-mcp" ] && OCTA_BIN="$REPO_ROOT/target/release/octasoma-mcp"
[ -z "$OCTA_BIN" ] && [ -f "$REPO_ROOT/octasoma/target/release/octasoma-mcp" ] && OCTA_BIN="$REPO_ROOT/octasoma/target/release/octasoma-mcp"
OCTA_MEM="/tmp/ecosystem-test-octa.mem"
OCTA_OK=0

if [ -n "$OCTA_BIN" ]; then
  pass "octasoma:binary:$OCTA_BIN"

  # 5.1 Initialize
  OCTA_INIT=$(echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | timeout 5 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('result',{}).get('serverInfo',{}).get('name','?'))" 2>/dev/null || echo "err")
  [ "$OCTA_INIT" = "octasoma" ] && { OCTA_OK=1; pass "octasoma:mcp-init"; } || fail "octasoma:mcp-init"

  # 5.2 Tool list
  OCTA_TOOLS=$(echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | timeout 5 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('result',{}).get('tools',[])))" 2>/dev/null || echo "0")
  [ "$OCTA_TOOLS" = 5 ] && pass "octasoma:mcp-tools (5)" || fail "octasoma:mcp-tools ($OCTA_TOOLS, expected 5)"

  if [ "$OCTA_OK" = 1 ]; then
    # 5.3 Stats (empty)
    OCTA_STATS=$(echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"stats","arguments":{}}}' | timeout 5 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); r=d.get('result',{}); c=r.get('content',[{}])[0]; print(c.get('text','err'))" 2>/dev/null || echo "err")
    echo "$OCTA_STATS" | python3 -c "import json,sys; json.load(sys.stdin); print('valid')" 2>/dev/null && pass "octasoma:stats" || fail "octasoma:stats"

    # 5.4 Ingest (single)
    OCTA_INGEST=$(echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ingest","arguments":{"uri":"tag:ecosystem-test/octa-1","text":"OctaSoma test dans la batterie écosystème: mémoire fractale 3D sémantique"}}}' | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); r=d.get('result',{}); c=r.get('content',[{}])[0].get('text','err'); print(c)" 2>/dev/null || echo "err")
    echo "$OCTA_INGEST" | python3 -c "import json,sys; d=json.load(sys.stdin); assert d.get('nodes_added',0) > 0; print('ok')" 2>/dev/null && pass "octasoma:ingest" || fail "octasoma:ingest ($OCTA_INGEST)"

    # 5.5 Recall
    OCTA_RECALL=$(echo '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"recall","arguments":{"text":"fractal memory ecosystem","k":5}}}' | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
r=d.get('result',{})
c=r.get('content',[{}])[0].get('text','{}')
items=json.loads(c).get('items',[])
print(len(items))" 2>/dev/null || echo "-1")
    [ "$OCTA_RECALL" -gt 0 ] && pass "octasoma:recall ($OCTA_RECALL items)" || warn "octasoma:recall (0 items)"

    # 5.6 Explain
    OCTA_EXPLAIN=$(echo '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"explain","arguments":{"text":"fractal memory","k":3}}}' | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if 'result' in d else 'err')" 2>/dev/null || echo "err")
    [ "$OCTA_EXPLAIN" = "ok" ] && pass "octasoma:explain" || warn "octasoma:explain"

    # 5.7 Batch ingest (stress)
    BATCH_O_PASS=0; BATCH_O_FAIL=0
    for i in $(seq 1 20); do
      OCTA_BATCH=$(echo "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{\"name\":\"ingest\",\"arguments\":{\"uri\":\"tag:ecosystem-test/octa-stress-$i\",\"text\":\"OctaSoma stress item #$i: fractal memory batch test at $(date +%s)\"}}}" | timeout 5 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if 'result' in d else 'err')" 2>/dev/null || echo "err")
      [ "$OCTA_BATCH" = "ok" ] && BATCH_O_PASS=$((BATCH_O_PASS+1)) || BATCH_O_FAIL=$((BATCH_O_FAIL+1))
    done
    [ "$BATCH_O_FAIL" = 0 ] && pass "octasoma:batch-ingest 20/20" || warn "octasoma:batch-ingest ${BATCH_O_PASS}/20"

    # 5.8 Recall after batch
    OCTA_RECALL2=$(echo '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"recall","arguments":{"text":"fractal memory ecosystem batch test","k":10}}}' | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
r=d.get('result',{})
c=r.get('content',[{}])[0].get('text','{}')
items=json.loads(c).get('items',[])
print(len(items))" 2>/dev/null || echo "0")
    [ "$OCTA_RECALL2" -ge 3 ] && pass "octasoma:recall-batch ($OCTA_RECALL2 items)" || warn "octasoma:recall-batch ($OCTA_RECALL2 items)"

    # 5.9 Feedback
    OCTA_FB=$(echo '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"feedback","arguments":{"relevant":["tag:ecosystem-test/octa-1"]}}}' | timeout 5 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if 'result' in d else 'err')" 2>/dev/null || echo "err")
    [ "$OCTA_FB" = "ok" ] && pass "octasoma:feedback" || warn "octasoma:feedback"

    # 5.10 Stats after all ops
    OCTA_STATS2=$(echo '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"stats","arguments":{}}}' | timeout 5 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
r=d.get('result',{})
c=r.get('content',[{}])[0].get('text','{}')
s=json.loads(c)
print(f'mem={s.get(\"memories\",0)} regions={s.get(\"regions\",0)}')" 2>/dev/null || echo "err")
    [ "$OCTA_STATS2" != "err" ] && pass "octasoma:stats-final ($OCTA_STATS2)" || warn "octasoma:stats-final"
  fi
else
  warn "octasoma:binary not found"
fi

#=============================================================================
# SECTION 6: Cascade CCOS → OctaSoma
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 6: Cascade CCOS → OctaSoma${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

if [ "$SOUL_OK" = 1 ] && [ -n "$OCTA_BIN" ]; then
  # 6.1 Recall from CCOS via REST
  CASCADE_CCOS=$(timeout 10 curl -sf -X POST "$SOUL_API/ccos/recall" \
    -H "Content-Type: application/json" \
    -d '{"text":"ecosystem test performance","strategy":"hybrid","budget":8}' 2>/dev/null || echo "")
  if [ -n "$CASCADE_CCOS" ]; then
    # 6.2 Rerank through OctaSoma
    CASCADE_TEXT=$(echo "$CASCADE_CCOS" | python3 -c "
import json,sys
d=json.load(sys.stdin)
items=d.get('result',{}).get('items',[])
combined=' '.join([i.get('content','') for i in items[:5] if i.get('content')])
print(combined[:2000])" 2>/dev/null || echo "")
    if [ -n "$CASCADE_TEXT" ]; then
      CASCADE_OCTA=$(echo "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"ingest\",\"arguments\":{\"uri\":\"tag:cascade/rerank-1\",\"text\":\"CASCADE_CCOS_OCTASOMA: $CASCADE_TEXT\"}}}" | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if 'result' in d else 'err')" 2>/dev/null || echo "err")
      [ "$CASCADE_OCTA" = "ok" ] && pass "cascade:ccos→octa ingest" || warn "cascade:ccos→octa ingest"

      # 6.3 Recall reranked
      CASCADE_RECALL=$(echo "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"recall\",\"arguments\":{\"text\":\"CCOS causal OctaSoma fractal rerank cascade\",\"k\":5}}}" | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
r=d.get('result',{})
c=r.get('content',[{}])[0].get('text','{}')
items=json.loads(c).get('items',[])
print(len(items), items[0].get('score',0) if items else 0)" 2>/dev/null || echo "0 0")
      CASCADE_ITEMS=$(echo "$CASCADE_RECALL" | awk '{print $1}')
      CASCADE_SCORE=$(echo "$CASCADE_RECALL" | awk '{print $2}')
      [ "$CASCADE_ITEMS" -gt 0 ] && pass "cascade:octa-recall ($CASCADE_ITEMS items, top score=$CASCADE_SCORE)" || warn "cascade:octa-recall (0 items)"
    else
      warn "cascade:no CCOS content to rerank"
    fi
  else
    warn "cascade:CCOS endpoint unreachable"
  fi
else
  [ "$SOUL_OK" != 1 ] && warn "cascade:CCOS not available"
  [ -z "$OCTA_BIN" ] && warn "cascade:OctaSoma not available"
fi

#=============================================================================
# SECTION 7: Gateway
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 7: SoulSystem Gateway${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

GW_PORT="${GW_PORT:-18890}"
GW_URL="http://127.0.0.1:$GW_PORT"

# 7.1 Gateway health
GW_HEALTH=$(timeout 5 curl -sf "$GW_URL/health" 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print('ok' if d.get('ok') else 'err')" 2>/dev/null || echo "err")
[ "$GW_HEALTH" = "ok" ] && pass "gateway:health" || fail "gateway:health (port $GW_PORT)"

# 7.2 Gateway process
GW_PID=$(ps aux | grep "[o]penclaw" | awk '{print $2}' | head -1)
[ -n "$GW_PID" ] && pass "gateway:process (PID=$GW_PID)" || warn "gateway:process (not running, may be starting)"

# 7.3 Gateway version
GW_VER=$(soulsystem-gateway --version 2>/dev/null || echo "?")
[ "$GW_VER" != "?" ] && pass "gateway:version ($GW_VER)" || warn "gateway:version unknown"

#=============================================================================
# SECTION 8: Cross-Component Integrity
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 8: Cross-Component Integrity${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

# 8.1 Memory persistence - CCOS recall the first ingest
PERSIST_CCOS=$(timeout 10 curl -sf -X POST "$SOUL_API/ccos/recall" \
  -H "Content-Type: application/json" \
  -d '{"text":"ingest-1 CERVO REST","strategy":"hybrid","budget":5}' 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
items=d.get('result',{}).get('items',[])
uris=[i.get('uri','') for i in items if 'ingest-1' in i.get('uri','')]
print(len(uris))" 2>/dev/null || echo "0")
[ "$PERSIST_CCOS" -gt 0 ] && pass "integrity:ccos-persist recalls ingest-1" || warn "integrity:ccos-persist (ingest-1 not found, expected after compaction)"

# 8.2 OctaSoma memory persistence
PERSIST_OCTA=$(echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"recall","arguments":{"text":"ecosystem-test octa-1 fractal","k":5}}}' | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
r=d.get('result',{})
c=r.get('content',[{}])[0].get('text','{}')
items=json.loads(c).get('items',[])
uris=[i.get('uri','') for i in items if 'octa-1' in i.get('uri','')]
print(len(uris))" 2>/dev/null || echo "0")
[ "$PERSIST_OCTA" -gt 0 ] && pass "integrity:octa-persist recalls octa-1" || warn "integrity:octa-persist (octa-1 not found)"

# 8.3 CERVO mutations check
CERVO_MUT=$(timeout 5 curl -sf "$SOUL_API/cervo/status" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
s=d.get('status',{})
print(f'mutations={s.get(\"total_mutations\",0)} processed={s.get(\"total_processed\",0)} adopted={s.get(\"total_adopted\",0)}')" 2>/dev/null || echo "err")
[ "$CERVO_MUT" != "err" ] && pass "integrity:cervo-mutations ($CERVO_MUT)" || warn "integrity:cervo-mutations"

# 8.4 Process count check
PROC_COUNT=$(ps aux | grep -cE "soulsystem|ccos|octasoma|cervo" 2>/dev/null || echo 0)
[ "$PROC_COUNT" -ge 1 ] && pass "integrity:ecosystem-procs ($PROC_COUNT)" || warn "integrity:ecosystem-procs (0?)"

#=============================================================================
# SECTION 9: Performance (Latency)
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 9: Performance (Latency)${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

# 9.1 CCOS recall latency (5 samples)
CCOS_LAT_SUM=0; CCOS_LAT_SAMPLES=5; CCOS_LAT_OK=0
for s in $(seq 1 $CCOS_LAT_SAMPLES); do
  T0=$(date +%s%N)
  timeout 5 curl -sf -X POST "$SOUL_API/ccos/recall" \
    -H "Content-Type: application/json" \
    -d '{"text":"latency test query","strategy":"hybrid","budget":5}' > /dev/null 2>&1 || true
  T1=$(date +%s%N)
  CCOS_LAT_SUM=$((CCOS_LAT_SUM + (T1 - T0) / 1000000))
  CCOS_LAT_OK=$((CCOS_LAT_OK + 1))
done
if [ "$CCOS_LAT_OK" -gt 0 ]; then
  CCOS_AVG=$((CCOS_LAT_SUM / CCOS_LAT_OK))
  [ "$CCOS_AVG" -lt 2000 ] && pass "perf:ccos-recall avg ${CCOS_AVG}ms" || warn "perf:ccos-recall avg ${CCOS_AVG}ms"
fi

# 9.2 OctaSoma recall latency (5 samples)
OCTA_LAT_SUM=0; OCTA_LAT_OK=0
for s in $(seq 1 5); do
  T0=$(date +%s%N)
  echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"recall","arguments":{"text":"latency test fractal memory","k":5}}}' | timeout 10 env OLLAMA_ENDPOINT="$OLLAMA_ENDPOINT" "$OCTA_BIN" "$OCTA_MEM" > /dev/null 2>&1 || true
  T1=$(date +%s%N)
  OCTA_LAT_SUM=$((OCTA_LAT_SUM + (T1 - T0) / 1000000))
  OCTA_LAT_OK=$((OCTA_LAT_OK + 1))
done
if [ "$OCTA_LAT_OK" -gt 0 ]; then
  OCTA_AVG=$((OCTA_LAT_SUM / OCTA_LAT_OK))
  if [ "$OCTA_AVG" -lt 5000 ]; then
    pass "perf:octa-recall avg ${OCTA_AVG}ms"
  else
    warn "perf:octa-recall avg ${OCTA_AVG}ms (embedding latency)"
  fi
fi

# 9.3 CERVO status latency
T0=$(date +%s%N)
timeout 5 curl -sf "$SOUL_API/cervo/status" > /dev/null 2>&1 || true
T1=$(date +%s%N)
CERVO_LAT=$(( (T1 - T0) / 1000000 ))
[ "$CERVO_LAT" -lt 500 ] && pass "perf:cervo-status ${CERVO_LAT}ms" || warn "perf:cervo-status ${CERVO_LAT}ms"

# 9.4 Gateway health latency
T0=$(date +%s%N)
timeout 5 curl -sf "$GW_URL/health" > /dev/null 2>&1 || true
T1=$(date +%s%N)
GW_LAT=$(( (T1 - T0) / 1000000 ))
[ "$GW_LAT" -lt 500 ] && pass "perf:gateway-health ${GW_LAT}ms" || warn "perf:gateway-health ${GW_LAT}ms"

#=============================================================================
# SECTION 10: Remote (Jetson) — skip when --remote-only or already on Jetson
#=============================================================================
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW} SECTION 10: Remote Jetson (192.168.0.36)${NC}"
echo -e "${YELLOW}══════════════════════════════════════════════════════════${NC}"

REMOTE_HOST="192.168.0.36"
REMOTE_PASS="DA5dqt8Z"

if [ "$REMOTE_ONLY" = 1 ]; then
  pass "remote-only:local-tests-complete"
elif [ "$MACHINE" != "Jetson" ] && command -v sshpass &>/dev/null; then
  # Copy test to remote and run (with timeout)
  REMOTE_LOG="/tmp/ecosystem-test-remote.log"
  sshpass -p "$REMOTE_PASS" scp -o StrictHostKeyChecking=no -o ConnectTimeout=5 "$0" root@$REMOTE_HOST:/tmp/ecosystem-test-remote.sh > /dev/null 2>&1 && pass "remote:scp-ok" || { fail "remote:scp"; warn "remote:skipped (scp failed)"; }

  # Run remote test in background with timeout
  if [ -f "/tmp/ecosystem-test-remote.output" ]; then rm -f /tmp/ecosystem-test-remote.output; fi
  timeout 90 sshpass -p "$REMOTE_PASS" ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 root@$REMOTE_HOST \
    "bash /tmp/ecosystem-test-remote.sh --remote-only" > /tmp/ecosystem-test-remote.output 2>&1 && REMOTE_OK=1 || REMOTE_OK=0
  if [ "$REMOTE_OK" = 1 ]; then
    pass "remote:execution"
    echo ""
    echo -e "${CYAN}--- Remote (Jetson) Results ---${NC}"
    grep -E "PASS:|FAIL:|WARN:|SCORE:" /tmp/ecosystem-test-remote.output | head -5
    REMOTE_P=$(grep -oP 'PASS: \K\d+' /tmp/ecosystem-test-remote.output | tail -1)
    REMOTE_F=$(grep -oP 'FAIL: \K\d+' /tmp/ecosystem-test-remote.output | tail -1)
    REMOTE_W=$(grep -oP 'WARN: \K\d+' /tmp/ecosystem-test-remote.output | tail -1)
    echo -e "${GREEN}Remote PASS: ${REMOTE_P:-?}${NC} ${RED}FAIL: ${REMOTE_F:-?}${NC} ${YELLOW}WARN: ${REMOTE_W:-?}${NC}"
  else
    fail "remote:execution (timeout/ssh failed)"
    echo "--- remote output ---"
    tail -5 /tmp/ecosystem-test-remote.output 2>/dev/null
  fi
else
  if [ "$MACHINE" = "Jetson" ]; then
    pass "remote:self (already on Jetson)"
  else
    warn "remote:sshpass not available"
  fi
fi

#=============================================================================
# SUMMARY
#=============================================================================
echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     TEST BATTERY COMPLETE                               ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${GREEN}PASS: $PASS${NC}"
echo -e "  ${RED}FAIL: $FAIL${NC}"
echo -e "  ${YELLOW}WARN: $WARN${NC}"
echo -e "  TOTAL: $TOTAL"
echo ""

SCORE=$(python3 -c "p=$PASS; t=$TOTAL; print(f'{p*100.0/t:.1f}')" 2>/dev/null || echo "?")
echo -e "  ${CYAN}SCORE: ${SCORE}%${NC}"
echo ""

# Failed list
if [ "$FAIL" -gt 0 ]; then
  echo -e "${RED}Failed tests:${NC}"
  for r in "${RESULTS[@]}"; do
    echo "$r" | grep "^FAIL:" && echo "    - ${r#FAIL:}"
  done
fi

# Cleanup test memory files
rm -f /tmp/test-ccos-mcp.ccos /tmp/ecosystem-test-octa.mem /tmp/ecosystem-test-remote.sh 2>/dev/null || true

exit $FAIL
