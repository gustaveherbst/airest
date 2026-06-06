#!/usr/bin/env bash
# Start aiREST per example folder, curl-test each endpoint, then stop the server.
#
# Prerequisites:
#   1. A .env file with OPENAI_API_KEY (and any other server vars; default port is 3300).
#   2. airest binary on PATH or built at ../target/{debug,release}/airest
#   3. Optional in your .env for auth examples:
#        AIREST_TEST_JWT=<valid RS256 JWT for jwt-protected-echo>
#        AIREST_TEST_OAUTH2_TOKEN=<valid access token for oauth2-protected>
#
# Usage:
#   ./examples/runall.sh
#   (prompts for the full path to your .env file)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ENV_FILE=""
PORT=3300
BASE_URL="http://localhost:${PORT}"
MCP_MOCK_URL="http://127.0.0.1:3100"
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/airest-runall.XXXXXX")"

AIREST_BIN=""
AIREST_PID=""
MCP_MOCK_PID=""

PASS=0
FAIL=0
SKIP=0
declare -a SUMMARY_LINES=()

if [[ -t 1 ]]; then
  RED=$'\033[0;31m'
  GREEN=$'\033[0;32m'
  YELLOW=$'\033[1;33m'
  CYAN=$'\033[0;36m'
  BOLD=$'\033[1m'
  DIM=$'\033[2m'
  NC=$'\033[0m'
else
  RED='' GREEN='' YELLOW='' CYAN='' BOLD='' DIM='' NC=''
fi

read_env_var() {
  local name="$1"
  local line value
  line="$(grep -E "^${name}=" "${ENV_FILE}" 2>/dev/null | tail -1 || true)"
  [[ -z "${line}" ]] && return 0
  value="${line#*=}"
  value="${value%$'\r'}"
  # Strip optional surrounding quotes.
  if [[ "${value}" == \"*\" && "${value}" == *\" ]]; then
    value="${value:1:${#value}-2}"
  elif [[ "${value}" == \'*\' && "${value}" == *\' ]]; then
    value="${value:1:${#value}-2}"
  fi
  printf '%s' "${value}"
}

load_env() {
  # Read only what the script needs. Do not export provider secrets into the
  # shell — airest must load them solely via --env-file (dotenvy skips vars
  # already present in the environment).
  PORT="$(read_env_var AIREST_PORT)"
  PORT="${PORT:-3300}"
  BASE_URL="http://localhost:${PORT}"
  AIREST_TEST_JWT="$(read_env_var AIREST_TEST_JWT)"
  AIREST_TEST_OAUTH2_TOKEN="$(read_env_var AIREST_TEST_OAUTH2_TOKEN)"
  echo "Env file: ${ENV_FILE}"
  echo "Using port ${PORT}"
}

airest_env() {
  # Clear config that airest reads from the environment so --env-file wins.
  # Force fast shutdown between folder runs (ignore .env drain settings).
  env \
    -u OPENAI_API_KEY -u OPENAI_BASE_URL \
    -u AZURE_OPENAI_API_KEY -u AZURE_OPENAI_ENDPOINT -u AZURE_OPENAI_API_VERSION \
    -u ANTHROPIC_API_KEY -u ANTHROPIC_BASE_URL -u ANTHROPIC_API_VERSION \
    -u GEMINI_API_KEY -u GEMINI_BASE_URL \
    -u GROK_API_KEY -u XAI_API_KEY -u GROK_BASE_URL \
    -u OLLAMA_API_KEY -u OLLAMA_BASE_URL \
    -u AIREST_PORT -u AIREST_API_KEY -u AIREST_API_DIR -u AIREST_PRODUCTION \
    -u AIREST_HOT_RELOAD -u AIREST_LOG_LEVEL \
    -u AIREST_GRACEFUL_SHUTDOWN -u AIREST_GRACEFUL_SHUTDOWN_SECS \
    AIREST_GRACEFUL_SHUTDOWN=false \
    "$@"
}

prompt_env_file() {
  local default="${SCRIPT_DIR}/.env"
  local input

  if [[ -t 0 ]]; then
    echo -n "Path to .env file (full path) [${default}]: "
    read -r input
  elif [[ -n "${AIREST_ENV_FILE:-}" ]]; then
    input="${AIREST_ENV_FILE}"
    echo "Using AIREST_ENV_FILE=${input}"
  else
    input="${default}"
    echo "Non-interactive session; using default ${default}"
  fi

  if [[ -z "${input}" ]]; then
    ENV_FILE="${default}"
  else
    # Expand leading ~ to home directory.
    if [[ "${input}" == "~/"* ]]; then
      input="${HOME}/${input:2}"
    elif [[ "${input}" == "~" ]]; then
      input="${HOME}"
    fi
    ENV_FILE="${input}"
  fi

  if [[ ! -f "${ENV_FILE}" ]]; then
    echo "${RED}Error:${NC} env file not found: ${ENV_FILE}"
    exit 1
  fi

  ENV_FILE="$(cd "$(dirname "${ENV_FILE}")" && pwd)/$(basename "${ENV_FILE}")"
  echo "Env file: ${ENV_FILE}"
}

find_airest() {
  if command -v airest >/dev/null 2>&1; then
    AIREST_BIN="$(command -v airest)"
  elif [[ -x "${REPO_ROOT}/target/release/airest" ]]; then
    AIREST_BIN="${REPO_ROOT}/target/release/airest"
  elif [[ -x "${REPO_ROOT}/target/debug/airest" ]]; then
    AIREST_BIN="${REPO_ROOT}/target/debug/airest"
  else
    echo "${RED}Error:${NC} airest binary not found (install to PATH or run cargo build)"
    exit 1
  fi
  echo "Using ${AIREST_BIN}"
}

pretty_json() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -m json.tool 2>/dev/null || cat
  else
    cat
  fi
}

port_open() {
  local host="$1"
  local port="$2"
  if command -v nc >/dev/null 2>&1; then
    nc -z "${host}" "${port}" 2>/dev/null
    return
  fi
  curl -sf --max-time 2 "http://${host}:${port}/health" >/dev/null 2>&1 \
    || curl -sf --max-time 2 "http://${host}:${port}/mcp" >/dev/null 2>&1
}

wait_for_url() {
  local url="$1"
  local label="$2"
  local attempts="${3:-60}"
  local i

  for ((i = 1; i <= attempts; i++)); do
    if curl -sf --max-time 2 "${url}" >/dev/null; then
      echo "${label} ready"
      return 0
    fi
    sleep 1
  done
  echo "${RED}Error:${NC} ${label} did not become ready at ${url}"
  return 1
}

kill_port_listeners() {
  local port="$1"
  local pids

  if ! command -v lsof >/dev/null 2>&1; then
    return 1
  fi

  pids="$(lsof -ti "tcp:${port}" -sTCP:LISTEN 2>/dev/null | tr '\n' ' ' || true)"
  pids="${pids%% }"
  [[ -z "${pids}" ]] && return 0

  echo "Killing listener(s) on port ${port}: ${pids}"
  # shellcheck disable=SC2086
  kill -9 ${pids} 2>/dev/null || true
  sleep 1
}

ensure_port_free() {
  local port="$1"
  local attempts="${2:-15}"

  if wait_for_port_closed "${port}" "${attempts}"; then
    return 0
  fi

  kill_port_listeners "${port}"
  wait_for_port_closed "${port}" 5
}

wait_for_port_closed() {
  local port="$1"
  local attempts="${2:-45}"
  local i

  for ((i = 1; i <= attempts; i++)); do
    if ! port_open "127.0.0.1" "${port}"; then
      return 0
    fi
    sleep 1
  done
  return 1
}

health_includes_path() {
  local path="$1"
  curl -sf --max-time 2 "${BASE_URL}/health" | python3 -c "
import json, sys
expected = sys.argv[1]
data = json.load(sys.stdin)
endpoints = data.get('meta', {}).get('endpoints', [])
sys.exit(0 if any(e.get('path') == expected for e in endpoints) else 1)
" "${path}" 2>/dev/null
}

wait_for_server_ready() {
  local folder="$1"
  local verify_path="$2"
  local log_file="$3"
  local attempts="${4:-60}"
  local i

  for ((i = 1; i <= attempts; i++)); do
    if ! kill -0 "${AIREST_PID}" 2>/dev/null; then
      echo "${RED}Error:${NC} aiREST (${folder}/) exited during startup"
      echo "  log: ${log_file}"
      tail -30 "${log_file}" 2>/dev/null || true
      return 1
    fi
    if health_includes_path "${verify_path}"; then
      echo "aiREST (${folder}/) ready — registered ${verify_path}"
      return 0
    fi
    sleep 1
  done

  echo "${RED}Error:${NC} aiREST (${folder}/) never registered ${verify_path}"
  echo "  log: ${log_file}"
  tail -30 "${log_file}" 2>/dev/null || true
  return 1
}

start_mcp_mock() {
  if port_open "127.0.0.1" "3100"; then
    echo "MCP mock already running on port 3100"
    return 0
  fi

  echo "${CYAN}Starting MCP mock:${NC} node mcp/mcp-mock-kb-remote.mjs"
  node "${SCRIPT_DIR}/mcp/mcp-mock-kb-remote.mjs" >"${LOG_DIR}/mcp-mock.log" 2>&1 &
  MCP_MOCK_PID=$!

  if ! wait_for_url "${MCP_MOCK_URL}/mcp" "MCP mock" 30; then
    echo "  log: ${LOG_DIR}/mcp-mock.log"
    tail -20 "${LOG_DIR}/mcp-mock.log" 2>/dev/null || true
    return 1
  fi
}

stop_mcp_mock() {
  if [[ -n "${MCP_MOCK_PID}" ]] && kill -0 "${MCP_MOCK_PID}" 2>/dev/null; then
    echo "Stopping MCP mock (pid ${MCP_MOCK_PID})"
    kill "${MCP_MOCK_PID}" 2>/dev/null || true
    wait "${MCP_MOCK_PID}" 2>/dev/null || true
  fi
  MCP_MOCK_PID=""
}

start_server() {
  local folder="$1"
  local verify_path="$2"
  local folder_path="${SCRIPT_DIR}/${folder}"
  local log_file="${LOG_DIR}/airest-${folder}.log"

  if [[ ! -d "${folder_path}" ]]; then
    echo "${RED}Error:${NC} folder not found: ${folder_path}"
    return 1
  fi

  if ! ensure_port_free "${PORT}" 15; then
    echo "${RED}Error:${NC} port ${PORT} is still in use; cannot start aiREST for ${folder}/"
    return 1
  fi

  echo "${CYAN}Starting aiREST:${NC} --env-file ${ENV_FILE} serve --folder ${folder_path}"
  airest_env "${AIREST_BIN}" --env-file "${ENV_FILE}" serve --folder "${folder_path}" >"${log_file}" 2>&1 &
  AIREST_PID=$!

  if ! wait_for_server_ready "${folder}" "${verify_path}" "${log_file}" 60; then
    stop_server
    return 1
  fi
}

stop_server() {
  if [[ -n "${AIREST_PID}" ]] && kill -0 "${AIREST_PID}" 2>/dev/null; then
    echo "Stopping aiREST (pid ${AIREST_PID})"
    kill -INT "${AIREST_PID}" 2>/dev/null || kill -TERM "${AIREST_PID}" 2>/dev/null || true
    local i
    for ((i = 1; i <= 10; i++)); do
      if ! kill -0 "${AIREST_PID}" 2>/dev/null; then
        break
      fi
      sleep 0.5
    done
    if kill -0 "${AIREST_PID}" 2>/dev/null; then
      echo "${YELLOW}Force stopping aiREST (pid ${AIREST_PID})${NC}"
      kill -9 "${AIREST_PID}" 2>/dev/null || true
      wait "${AIREST_PID}" 2>/dev/null || true
    fi
  fi
  AIREST_PID=""
  if ! wait_for_port_closed "${PORT}" 5; then
    kill_port_listeners "${PORT}"
    wait_for_port_closed "${PORT}" 5 || true
  fi
}

cleanup() {
  stop_server
  stop_mcp_mock
}
trap cleanup EXIT INT TERM

run_test() {
  local id="$1"
  local request_desc="$2"
  shift 2

  local body_file
  body_file="$(mktemp)"
  local http_code

  if ! http_code="$(curl -sS -o "${body_file}" -w "%{http_code}" "$@")"; then
    echo "${RED}FAIL${NC}  ${id}  (curl error)"
    echo "  ${DIM}Request:${NC}"
    echo "    ${request_desc}"
    echo "  ${DIM}Response:${NC}"
    echo "    (connection failed)"
    SUMMARY_LINES+=("${RED}FAIL${NC}  ${id}  (curl error)")
    FAIL=$((FAIL + 1))
    rm -f "${body_file}"
    return
  fi

  if [[ "${http_code}" =~ ^2[0-9][0-9]$ ]]; then
    echo "${GREEN}PASS${NC}  ${id}  (${http_code})"
    SUMMARY_LINES+=("${GREEN}PASS${NC}  ${id}  (${http_code})")
    PASS=$((PASS + 1))
  else
    echo "${RED}FAIL${NC}  ${id}  (${http_code})"
    echo "  ${DIM}Request:${NC}"
    echo "    ${request_desc}"
    echo "  ${DIM}Response:${NC}"
    sed 's/^/    /' "${body_file}" | pretty_json
    SUMMARY_LINES+=("${RED}FAIL${NC}  ${id}  (${http_code})")
    FAIL=$((FAIL + 1))
  fi

  rm -f "${body_file}"
}

skip_test() {
  local id="$1"
  local reason="$2"
  echo "${YELLOW}SKIP${NC}  ${id}  (${reason})"
  SUMMARY_LINES+=("${YELLOW}SKIP${NC}  ${id}  (${reason})")
  SKIP=$((SKIP + 1))
}

run_folder() {
  local folder="$1"
  local verify_path="$2"
  shift 2

  echo
  echo "${BOLD}=== ${folder}/ ===${NC}"

  if [[ "${folder}" == "mcp" ]]; then
    start_mcp_mock || echo "${YELLOW}Warning:${NC} MCP mock unavailable; HTTP/SSE tests will be skipped."
  fi

  if ! start_server "${folder}" "${verify_path}"; then
    echo "${YELLOW}Skipping tests for ${folder}/ (server failed to start)${NC}"
    [[ "${folder}" == "mcp" ]] && stop_mcp_mock
    return 1
  fi

  while [[ $# -gt 0 ]]; do
    local test_fn="$1"
    shift
    "${test_fn}"
  done

  stop_server

  if [[ "${folder}" == "mcp" ]]; then
    stop_mcp_mock
  fi
}

# --- per-endpoint test functions ---

test_legal_contract() {
  run_test "legal/contract-risk-analyzer" \
    "POST ${BASE_URL}/v1/analyze-contract-risk  Content-Type: application/json  body: contractText+jurisdiction+riskTolerance" \
    -X POST "${BASE_URL}/v1/analyze-contract-risk" \
    -H "Content-Type: application/json" \
    -d '{"contractText":"This agreement is entered into between Company A and Company B and includes payment terms, termination clauses, and confidentiality obligations sufficient for review.","jurisdiction":"Oklahoma","riskTolerance":"medium"}'
}

test_legal_nda() {
  run_test "legal/nda-risk-check" \
    "POST ${BASE_URL}/v1/check-nda-risk  Content-Type: application/json  body: ndaText+partyRole+jurisdiction" \
    -X POST "${BASE_URL}/v1/check-nda-risk" \
    -H "Content-Type: application/json" \
    -d '{"ndaText":"This mutual non-disclosure agreement governs confidential information shared between the parties for evaluating a potential business relationship and includes term, exclusions, and return obligations.","partyRole":"mutual","jurisdiction":"Delaware"}'
}

test_analytics_sentiment() {
  run_test "analytics/sentiment-analyzer" \
    "POST ${BASE_URL}/v1/analyze-sentiment  Content-Type: application/json  body: text" \
    -X POST "${BASE_URL}/v1/analyze-sentiment" \
    -H "Content-Type: application/json" \
    -d '{"text":"I love this product!"}'
}

test_analytics_quick_sentiment() {
  run_test "analytics/quick-sentiment" \
    "GET ${BASE_URL}/v1/quick-sentiment?text=I+love+this+product!" \
    -G "${BASE_URL}/v1/quick-sentiment" \
    --data-urlencode "text=I love this product!"
}

test_analytics_summarizer() {
  run_test "analytics/text-summarizer" \
    "POST ${BASE_URL}/v1/summarize-text  Content-Type: application/json  body: text+maxBullets+audience" \
    -X POST "${BASE_URL}/v1/summarize-text" \
    -H "Content-Type: application/json" \
    -d '{"text":"Quarterly revenue grew 12 percent driven by enterprise subscriptions while support costs increased due to onboarding volume and two major outages in March.","maxBullets":3,"audience":"executive"}'
}

test_healthcare_clinical() {
  run_test "healthcare/clinical-note-summary" \
    "POST ${BASE_URL}/v1/summarize-clinical-note  Content-Type: application/json  body: clinicalNote+patientName+mrn+dateOfBirth" \
    -X POST "${BASE_URL}/v1/summarize-clinical-note" \
    -H "Content-Type: application/json" \
    -d '{"clinicalNote":"Patient presents with improved mobility after physical therapy. Vitals stable. Continue current plan.","patientName":"Jane Doe","mrn":"12345","dateOfBirth":"1980-01-01"}'
}

test_finance_fraud() {
  run_test "finance/payment-fraud-check" \
    "POST ${BASE_URL}/v1/check-payment-fraud  Content-Type: application/json  body: amount+currency+merchantId+cardLast4" \
    -X POST "${BASE_URL}/v1/check-payment-fraud" \
    -H "Content-Type: application/json" \
    -d '{"amount":150.0,"currency":"USD","merchantId":"merch_abc123","cardLast4":"4242"}'
}

test_content_headline() {
  run_test "content/headline-generator" \
    "POST ${BASE_URL}/v1/generate-headline  Content-Type: application/json  body: product+audience+tone+count" \
    -X POST "${BASE_URL}/v1/generate-headline" \
    -H "Content-Type: application/json" \
    -d '{"product":"aiREST turns YAML into production AI REST APIs","audience":"backend engineers","tone":"professional","count":3}'
}

test_support_triage() {
  run_test "support/ticket-triage" \
    "POST ${BASE_URL}/v1/triage-support-ticket  Content-Type: application/json  body: subject+body+customerTier" \
    -X POST "${BASE_URL}/v1/triage-support-ticket" \
    -H "Content-Type: application/json" \
    -d '{"subject":"Cannot access my account","body":"I reset my password twice and still cannot log in.","customerTier":"premium"}'
}

test_support_reply() {
  run_test "support/reply-suggester" \
    "POST ${BASE_URL}/v1/suggest-support-reply  Content-Type: application/json  body: customerMessage+tone" \
    -X POST "${BASE_URL}/v1/suggest-support-reply" \
    -H "Content-Type: application/json" \
    -d '{"customerMessage":"My invoice looks wrong and I was charged twice this month.","tone":"empathetic"}'
}

test_support_escalation() {
  run_test "support/escalation-advisor" \
    "POST ${BASE_URL}/v1/advise-support-escalation  Content-Type: application/json  body: subject+body+priority+customerTier+previousAttempts" \
    -X POST "${BASE_URL}/v1/advise-support-escalation" \
    -H "Content-Type: application/json" \
    -d '{"subject":"Production outage affecting billing","body":"Our checkout has been failing for two hours and enterprise customers cannot pay.","priority":"critical","customerTier":"enterprise","previousAttempts":1}'
}

test_mcp_hf() {
  run_test "mcp/kb-ticket-search-hf" \
    "POST ${BASE_URL}/v1/search-support-kb  Content-Type: application/json  body: query  (remote MCP HTTP https://hf.co/mcp)" \
    -X POST "${BASE_URL}/v1/search-support-kb" \
    -H "Content-Type: application/json" \
    -d '{"query":"how to authenticate API requests"}'
}

test_auth_jwt() {
  if [[ -n "${AIREST_TEST_JWT:-}" ]]; then
    run_test "auth/jwt-protected-echo" \
      "POST ${BASE_URL}/v1/auth/jwt-echo  Content-Type: application/json  Authorization: Bearer <AIREST_TEST_JWT>  body: message" \
      -X POST "${BASE_URL}/v1/auth/jwt-echo" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer ${AIREST_TEST_JWT}" \
      -d '{"message":"hello from JWT caller"}'
  else
    skip_test "auth/jwt-protected-echo" \
      "set AIREST_TEST_JWT in your .env to a valid RS256 JWT (issuer https://auth.example.com, audience airest-api)"
  fi
}

test_auth_oauth2() {
  if [[ -n "${AIREST_TEST_OAUTH2_TOKEN:-}" ]]; then
    run_test "auth/oauth2-protected" \
      "POST ${BASE_URL}/v1/auth/oauth2-echo  Content-Type: application/json  Authorization: Bearer <AIREST_TEST_OAUTH2_TOKEN>  body: prompt" \
      -X POST "${BASE_URL}/v1/auth/oauth2-echo" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer ${AIREST_TEST_OAUTH2_TOKEN}" \
      -d '{"prompt":"hello from oauth2 caller"}'
  else
    skip_test "auth/oauth2-protected" \
      "set AIREST_TEST_OAUTH2_TOKEN in your .env to a valid access token (introspection via AIREST_OAUTH2_* env vars on server)"
  fi
}

test_auth_gateway() {
  run_test "auth/trust-gateway-echo" \
    "POST ${BASE_URL}/v1/auth/gateway-echo  Content-Type: application/json  x-user-id  x-tenant-id  body: text" \
    -X POST "${BASE_URL}/v1/auth/gateway-echo" \
    -H "Content-Type: application/json" \
    -H "x-user-id: test-user-001" \
    -H "x-tenant-id: test-tenant-001" \
    -d '{"text":"hello from trusted gateway"}'
}

test_mcp_local() {
  run_test "mcp/kb-ticket-search-local" \
    "POST ${BASE_URL}/v1/search-support-kb-local  Content-Type: application/json  body: query  (local Deno tool)" \
    -X POST "${BASE_URL}/v1/search-support-kb-local" \
    -H "Content-Type: application/json" \
    -d '{"query":"customer cannot reset password"}'
}

test_mcp_http() {
  if port_open "127.0.0.1" "3100"; then
    run_test "mcp/kb-ticket-search-http" \
      "POST ${BASE_URL}/v1/search-support-kb-http  Content-Type: application/json  body: query  (remote MCP HTTP ${MCP_MOCK_URL}/mcp)" \
      -X POST "${BASE_URL}/v1/search-support-kb-http" \
      -H "Content-Type: application/json" \
      -d '{"query":"customer cannot reset password"}'
  else
    skip_test "mcp/kb-ticket-search-http" "MCP mock not running on port 3100"
  fi
}

test_mcp_sse() {
  if port_open "127.0.0.1" "3100"; then
    run_test "mcp/kb-ticket-search-sse" \
      "POST ${BASE_URL}/v1/search-support-kb-sse  Content-Type: application/json  body: query  (remote MCP SSE ${MCP_MOCK_URL}/mcp/sse)" \
      -X POST "${BASE_URL}/v1/search-support-kb-sse" \
      -H "Content-Type: application/json" \
      -d '{"query":"customer cannot reset password"}'
  else
    skip_test "mcp/kb-ticket-search-sse" "MCP mock not running on port 3100"
  fi
}

main() {
  echo "${BOLD}aiREST example smoke tests${NC}"
  echo "Logs: ${LOG_DIR}"
  echo

  prompt_env_file
  load_env
  find_airest
  echo "Base URL: ${BASE_URL}"
  echo

  run_folder legal "/v1/analyze-contract-risk" test_legal_contract test_legal_nda
  run_folder analytics "/v1/quick-sentiment" test_analytics_sentiment test_analytics_quick_sentiment test_analytics_summarizer
  run_folder healthcare "/v1/summarize-clinical-note" test_healthcare_clinical
  run_folder finance "/v1/check-payment-fraud" test_finance_fraud
  run_folder content "/v1/generate-headline" test_content_headline
  run_folder support "/v1/triage-support-ticket" test_support_triage test_support_reply test_support_escalation
  run_folder auth "/v1/auth/gateway-echo" test_auth_jwt test_auth_oauth2 test_auth_gateway
  run_folder mcp "/v1/search-support-kb" test_mcp_hf test_mcp_local test_mcp_http test_mcp_sse

  # Prevent trap from trying to stop already-stopped processes again.
  AIREST_PID=""
  MCP_MOCK_PID=""

  echo
  echo "${BOLD}========================================${NC}"
  echo "${BOLD}Final results${NC}"
  echo "${BOLD}========================================${NC}"
  echo "${GREEN}${PASS} passed${NC}, ${RED}${FAIL} failed${NC}, ${YELLOW}${SKIP} skipped${NC}  (total $((PASS + FAIL + SKIP)))"
  echo
  for line in "${SUMMARY_LINES[@]}"; do
    echo "  ${line}"
  done
  echo
  echo "Server logs: ${LOG_DIR}"
  echo

  if [[ "${FAIL}" -gt 0 ]]; then
    exit 1
  fi
}

main "$@"
