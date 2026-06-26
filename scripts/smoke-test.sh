#!/usr/bin/env bash
# Gem Finder post-deployment smoke test
# Usage: ./scripts/smoke-test.sh https://yourdomain.com

set -euo pipefail

BASE_URL="${1:-}"

# ── colours ────────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
RED='\033[0;31m'
BOLD='\033[1m'
RESET='\033[0m'

pass() { echo -e "  ${GREEN}✓${RESET}  $1"; }
fail() { echo -e "  ${RED}✗${RESET}  $1"; FAILURES=$((FAILURES + 1)); }

FAILURES=0

# ── preflight ──────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}Gem Finder Smoke Test${RESET}"
echo "──────────────────────────────────────────"

if [[ -z "$BASE_URL" ]]; then
  echo -e "${RED}Error:${RESET} base URL required."
  echo "  Usage: $0 https://yourdomain.com"
  exit 1
fi

# Strip trailing slash
BASE_URL="${BASE_URL%/}"

echo "  Target: $BASE_URL"
echo ""

# Confirm server is reachable before running tests
echo -e "${BOLD}Preflight${RESET}"
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$BASE_URL/" 2>/dev/null || echo "000")
if [[ "$HTTP_CODE" == "000" ]]; then
  echo -e "  ${RED}✗${RESET}  Server unreachable at $BASE_URL — aborting."
  exit 1
fi
pass "Server is reachable ($HTTP_CODE)"
echo ""

# ── helpers ────────────────────────────────────────────────────────────────────

# check_status <label> <url> <method> <body> <expected_codes...>
check_status() {
  local label="$1"
  local url="$2"
  local method="$3"
  local body="$4"
  shift 4
  local expected=("$@")

  local curl_args=(-s -o /dev/null -w "%{http_code}" --max-time 15 -X "$method")
  if [[ -n "$body" ]]; then
    curl_args+=(-H "Content-Type: application/json" -d "$body")
  fi

  local code
  code=$(curl "${curl_args[@]}" "$url" 2>/dev/null || echo "000")

  for expected_code in "${expected[@]}"; do
    if [[ "$code" == "$expected_code" ]]; then
      pass "$label → $code"
      return 0
    fi
  done

  local expected_str
  expected_str=$(printf "%s or " "${expected[@]}")
  expected_str="${expected_str% or }"
  fail "$label → $code (expected $expected_str)"
}

# check_json <label> <url>  — GET, expect 200 + JSON array/object
check_json() {
  local label="$1"
  local url="$2"

  local body
  local code
  body=$(curl -s -o /tmp/_smoke_body -w "%{http_code}" --max-time 15 "$url" 2>/dev/null || echo "000")
  code="$body"

  if [[ "$code" != "200" ]]; then
    fail "$label → $code (expected 200)"
    return
  fi

  local content
  content=$(cat /tmp/_smoke_body)
  if echo "$content" | python3 -c "import sys, json; json.load(sys.stdin)" 2>/dev/null; then
    pass "$label → $code (valid JSON)"
  else
    fail "$label → $code (invalid JSON body)"
  fi
}

# ── tests ──────────────────────────────────────────────────────────────────────
echo -e "${BOLD}Core pages${RESET}"
check_status "GET /" "$BASE_URL/" GET "" 200

echo ""
echo -e "${BOLD}Public API${RESET}"
check_json   "GET /api/gems"      "$BASE_URL/api/gems"
check_json   "GET /api/acclaimed" "$BASE_URL/api/acclaimed"
check_json   "GET /api/wildcards" "$BASE_URL/api/wildcards"
check_status "GET /api/admin/status" "$BASE_URL/api/admin/status" GET "" 200

echo ""
echo -e "${BOLD}Auth endpoints${RESET}"
check_status \
  "POST /api/auth/magic (smoke@test.invalid)" \
  "$BASE_URL/api/auth/magic" \
  POST \
  '{"email":"smoke@test.invalid"}' \
  200 422

check_status "GET /api/watchlist (no auth → 401)" "$BASE_URL/api/watchlist" GET "" 401
check_status "GET /api/user/me  (no auth → 401)" "$BASE_URL/api/user/me"   GET "" 401

# ── summary ────────────────────────────────────────────────────────────────────
echo ""
echo "──────────────────────────────────────────"
TOTAL=8
PASSED=$((TOTAL - FAILURES))
if [[ "$FAILURES" -eq 0 ]]; then
  echo -e "  ${GREEN}${BOLD}All $TOTAL tests passed.${RESET}"
  exit 0
else
  echo -e "  ${RED}${BOLD}$FAILURES of $TOTAL tests FAILED.${RESET}"
  exit 1
fi
