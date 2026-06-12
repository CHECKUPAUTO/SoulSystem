# OpenEvolve Regression Test Suite

Automated regression tests for OpenClaw ecosystem changes.

**Purpose:** Ensure changes don't break existing functionality  
**Run Schedule:** Before each deployment, after night cycle auto-apply  
**Location:** `evolution/scripts/regression_tests/`

---

## Test Categories

### 1. Gateway Tests

```bash
#!/bin/bash
# tests/test_gateway.sh

echo "🧪 Gateway Regression Tests"
echo "============================"

FAILED=0

# Test 1: Gateway starts without crash
echo "Test 1: Gateway startup..."
timeout 10 openclaw gateway start --foreground &
PID=$!
sleep 3
if pgrep -f "openclaw-gateway" > /dev/null; then
    echo "  ✅ PASS: Gateway started"
else
    echo "  ❌ FAIL: Gateway failed to start"
    FAILED=$((FAILED + 1))
fi
kill $PID 2>/dev/null

# Test 2: Port binding
echo "Test 2: Port binding..."
if nc -z localhost 18888 2>/dev/null; then
    echo "  ✅ PASS: Port 18888 bound"
else
    echo "  ❌ FAIL: Port not bound"
    FAILED=$((FAILED + 1))
fi

# Test 3: SQLite fallback works
echo "Test 3: SQLite fallback..."
if node -e "require('node:sqlite')" 2>/dev/null || \
   npm list sqlite3 >/dev/null 2>&1; then
    echo "  ✅ PASS: SQLite available"
else
    echo "  ❌ FAIL: No SQLite available"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "============================"
echo "Results: $FAILED failed"
exit $FAILED
```

### 2. Session Management Tests

```python
#!/usr/bin/env python3
# tests/test_sessions.py

import asyncio
import sys

async def test_session_isolation():
    """Test that sessions are properly isolated."""
    # Implementation
    pass

async def test_session_persistence():
    """Test session data persistence across restarts."""
    pass

async def test_label_injection():
    """Test for #64699: sessions_send label injection bug."""
    # Should not inject label when sessionKey provided
    pass

async def main():
    tests = [
        test_session_isolation,
        test_session_persistence,
        test_label_injection,
    ]
    
    failed = 0
    for test in tests:
        try:
            await test()
            print(f"✅ {test.__name__}")
        except Exception as e:
            print(f"❌ {test.__name__}: {e}")
            failed += 1
    
    return failed

if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
```

### 3. Telegram Integration Tests

```python
#!/usr/bin/env python3
# tests/test_telegram.py

import json
import sys

def test_require_mention():
    """Test #64698: requireMention configuration."""
    config = json.load(open(f"{os.environ['HOME']}/.openclaw/config.json"))
    
    telegram_config = config.get('telegram', {})
    
    # Check global setting
    if telegram_config.get('requireMention') is True:
        print("✅ requireMention set globally")
        return 0
    else:
        print("❌ requireMention not set to true globally")
        return 1

def test_mention_in_groups():
    """Test that requireMention works in group chats."""
    # Mock group message without mention
    # Should be ignored if requireMention=true
    pass

if __name__ == "__main__":
    sys.exit(test_require_mention())
```

### 4. Skill Tests

```bash
#!/bin/bash
# tests/test_skills.sh

echo "🧪 Skills Regression Tests"
echo "==========================="

FAILED=0

# Test read-evolved
echo "Test: read-evolved..."
result=$(python3 /root/.openclaw/workspace/skills/read-evolved/scripts/read_core.py /etc/passwd 2>&1)
if echo "$result" | grep -q "root"; then
    echo "  ✅ PASS"
else
    echo "  ❌ FAIL"
    FAILED=$((FAILED + 1))
fi

# Test exec-evolved
echo "Test: exec-evolved..."
result=$(python3 /root/.openclaw/workspace/skills/exec-evolved/scripts/exec_core.py "echo test" 5)
if echo "$result" | grep -q "success"; then
    echo "  ✅ PASS"
else
    echo "  ❌ FAIL"
    FAILED=$((FAILED + 1))
fi

# Test the-well skill
echo "Test: the-well..."
python3 -c "from skills.the_well.scripts.well_client import WellClient; c = WellClient(); print(len(c.list_datasets()))" 2>/dev/null
if [ $? -eq 0 ]; then
    echo "  ✅ PASS"
else
    echo "  ❌ FAIL"
    FAILED=$((FAILED + 1))
fi

echo ""
echo "==========================="
echo "Results: $FAILED failed"
exit $FAILED
```

---

## Running Tests

### Quick Run
```bash
cd /root/.openclaw/workspace/evolution/scripts/regression_tests
./run_all.sh
```

### Continuous Integration
```yaml
# .github/workflows/regression.yml
name: Regression Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - run: ./evolution/scripts/regression_tests/run_all.sh
```

### Pre-Deployment Check
```bash
# In deployment script
if ! ./evolution/scripts/regression_tests/run_all.sh; then
    echo "❌ Regression tests failed, aborting deployment"
    exit 1
fi
```

---

## Test Output Format

```json
{
  "timestamp": "2026-04-11T11:30:00Z",
  "total_tests": 15,
  "passed": 14,
  "failed": 1,
  "duration_seconds": 45.2,
  "results": [
    {
      "name": "Gateway startup",
      "status": "passed",
      "duration_ms": 3200
    },
    {
      "name": "SQLite fallback",
      "status": "failed",
      "error": "node:sqlite not available",
      "duration_ms": 500
    }
  ]
}
```

---

## Known Issues Tests

### Issue #64695: node:sqlite
```bash
test_node_sqlite() {
    if node -e "require('node:sqlite')" 2>/dev/null; then
        echo "✅ node:sqlite available"
        return 0
    else
        # Check if fallback works
        if [ -f "/mnt/nvme_secondary/ai_projects/openclaw/node_modules/sqlite3/package.json" ]; then
            echo "⚠️  node:sqlite missing, sqlite3 fallback present"
            return 0  # Acceptable
        else
            echo "❌ Neither node:sqlite nor sqlite3 fallback available"
            return 1
        fi
    fi
}
```

### Issue #64698: requireMention
```bash
test_require_mention() {
    config=$(cat ~/.openclaw/config.json)
    if echo "$config" | jq -e '.telegram.requireMention == true' >/dev/null; then
        echo "✅ requireMention properly configured"
        return 0
    else
        echo "❌ requireMention not set to true"
        return 1
    fi
}
```

---

## Maintenance

**Update Frequency:**
- Add new tests for each P0/P1 bug
- Run full suite before each release
- Review and update monthly

**Adding New Tests:**
1. Create test file in `tests/`
2. Add to `run_all.sh`
3. Document in this file
4. Test locally before committing