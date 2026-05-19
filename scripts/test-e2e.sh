#!/usr/bin/env bash
# E2E smoke test — verifies multi-tenant isolation, audit trail, full CRUD
# Usage: ./scripts/test-e2e.sh  (backend must be running, demo data seeded)

set -euo pipefail

BASE_URL="${NEXUSMIND_BASE_URL:-http://localhost:8080}"

SARAH_KEY="nm_demo_acme_sarah"
MARCUS_KEY="nm_demo_acme_marcus"
TECHSTARTUP_KEY="nm_demo_techstartup_admin"
ADMIN_KEY="nm_demo_acme_admin"

PASS=0
FAIL=0

pass() { echo "    PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "    FAIL: $1"; FAIL=$((FAIL + 1)); }

auth() {
  local key="$1"; shift
  curl -sf -H "Authorization: Bearer $key" "$@"
}

echo "==> Health check"
health=$(auth "$ADMIN_KEY" "$BASE_URL/v1/health")
echo "$health" | grep -q '"status":"ok"' && pass "backend healthy" || fail "backend not healthy"

echo ""
echo "==> Store memory (Sarah, Acme Corp)"
store=$(auth "$SARAH_KEY" -X POST "$BASE_URL/v1/memory/store" \
  -H "Content-Type: application/json" \
  -d '{"content":"E2E test: snake_case for all REST endpoints","project":"e2e","tool":"test-script","tags":["convention","api"]}')
MEMORY_ID=$(echo "$store" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
[ -n "$MEMORY_ID" ] && pass "stored memory id=$MEMORY_ID" || fail "store failed"

echo ""
echo "==> Search memory (Marcus, same org — should find Sarah's entry)"
results=$(auth "$MARCUS_KEY" -X POST "$BASE_URL/v1/memory/search" \
  -H "Content-Type: application/json" \
  -d '{"query":"snake_case REST endpoints","limit":5}')
count=$(echo "$results" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
[ "$count" -ge 1 ] && pass "Marcus found $count result(s) from Sarah" || fail "Marcus got 0 results (isolation broken or search broken)"

echo ""
echo "==> Org isolation — TechStartup should NOT see Acme's memory"
ts_results=$(auth "$TECHSTARTUP_KEY" -X POST "$BASE_URL/v1/memory/search" \
  -H "Content-Type: application/json" \
  -d '{"query":"snake_case REST endpoints","limit":5}')
ts_count=$(echo "$ts_results" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
[ "$ts_count" -eq 0 ] && pass "TechStartup sees 0 results (isolation correct)" || fail "TechStartup sees $ts_count results (ISOLATION LEAK!)"

echo ""
echo "==> Audit log has entries (admin)"
audit=$(auth "$ADMIN_KEY" "$BASE_URL/v1/audit?limit=10")
audit_count=$(echo "$audit" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
[ "$audit_count" -ge 1 ] && pass "audit log has $audit_count entries" || fail "audit log empty"

echo ""
echo "==> Delete memory (Sarah)"
auth "$SARAH_KEY" -X DELETE "$BASE_URL/v1/memory/$MEMORY_ID" > /dev/null
pass "deleted $MEMORY_ID"

echo ""
echo "==> Verify deleted memory not in search"
after=$(auth "$SARAH_KEY" -X POST "$BASE_URL/v1/memory/search" \
  -H "Content-Type: application/json" \
  -d "{\"query\":\"$MEMORY_ID\",\"limit\":5}")
after_count=$(echo "$after" | python3 -c "import sys,json; d=json.load(sys.stdin); print(sum(1 for m in d if m.get('id')=='$MEMORY_ID'))")
[ "$after_count" -eq 0 ] && pass "deleted memory not found in search" || fail "deleted memory still appearing"

echo ""
echo "========================================"
echo "Results: $PASS passed, $FAIL failed"
echo "========================================"

[ "$FAIL" -eq 0 ] || exit 1
