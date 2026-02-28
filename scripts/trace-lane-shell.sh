#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 6 ]]; then
  echo "usage: $0 <lane_id> <profile> <repo_root> <trace_root> <trace_server_addr> <role> [lane_mode]" >&2
  echo "roles: lane | observer | server" >&2
  echo "lane_mode: interactive | runner (default: interactive)" >&2
  exit 2
fi

LANE_ID="$1"
PROFILE="$2"
REPO_ROOT="$3"
TRACE_ROOT="$4"
TRACE_SERVER_ADDR="$5"
ROLE="$6"
LANE_MODE="${7:-interactive}"

TRACE_API_BASE_URL="${TRACE_API_BASE_URL:-http://${TRACE_SERVER_ADDR}}"
SHELL_BIN="${SHELL:-/bin/zsh}"

export TRACE_ROOT TRACE_SERVER_ADDR TRACE_API_BASE_URL
export LANE_ID
export LANE_PROFILE="$PROFILE"

cd "$REPO_ROOT"

validate_lane_mode() {
  case "$1" in
    interactive|runner)
      ;;
    *)
      echo "invalid lane mode: $1 (allowed: interactive, runner)" >&2
      exit 2
      ;;
  esac
}

print_common_banner() {
  echo "TRACE_ROOT=$TRACE_ROOT"
  echo "TRACE_SERVER_ADDR=$TRACE_SERVER_ADDR"
  echo "TRACE_API_BASE_URL=$TRACE_API_BASE_URL"
  echo "LANE_ID=$LANE_ID"
  echo "LANE_PROFILE=$LANE_PROFILE"
  echo "LANE_MODE=$LANE_MODE"
}

print_lane_hints() {
  cat <<'EOF_HINTS'
Manual lane flow (copy/paste and replace IDs):
  TASK_ID=TASK-DEMO
  RUN_ID="${LANE_ID}-run-$(date +%s)"

  curl -sS -X POST "$TRACE_API_BASE_URL/tasks/$TASK_ID/claim" \
    -H 'content-type: application/json' \
    -d "{\"worker_id\":\"$LANE_ID\",\"expected_epoch\":0,\"title\":\"$TASK_ID\"}"

  curl -sS -X POST "$TRACE_API_BASE_URL/tasks/$TASK_ID/runs/start" \
    -H 'content-type: application/json' \
    -d "{\"run_id\":\"$RUN_ID\",\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"profile\":\"$LANE_PROFILE\",\"provider\":\"openai\",\"model\":\"gpt-5-$LANE_PROFILE\"}"

  curl -sS -X POST "$TRACE_API_BASE_URL/tasks/$TASK_ID/runs/$RUN_ID/output" \
    -H 'content-type: application/json' \
    -d "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"stream\":\"stdout\",\"encoding\":\"utf8\",\"chunk\":\"hello from $LANE_ID\",\"chunk_index\":0,\"final\":true}"

  curl -sS -X POST "$TRACE_API_BASE_URL/tasks/$TASK_ID/runs/$RUN_ID/candidates" \
    -H 'content-type: application/json' \
    -d "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"candidate_id\":\"C-$RUN_ID\"}"

  curl -sS -X POST "$TRACE_API_BASE_URL/tasks/$TASK_ID/release" \
    -H 'content-type: application/json' \
    -d "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1}"
EOF_HINTS
}

print_runner_hints() {
  cat <<'EOF_HINTS'
Runner mode env knobs:
  TRACE_RUNNER_TASK_COUNT        number of tasks to emit (default: 1)
  TRACE_RUNNER_TASK_PREFIX       task prefix (default: TASK-SMOKE)
  TRACE_RUNNER_NONCE             run nonce (default: unix timestamp)
  TRACE_RUNNER_VERDICT           verdict payload value (default: pass)
  TRACE_RUNNER_RETRY_ATTEMPTS    request retry attempts (default: 20)
  TRACE_RUNNER_RETRY_DELAY_SEC   retry delay in seconds (default: 1)
  TRACE_RUNNER_READY_TIMEOUT_SEC api readiness wait timeout (default: 30)
  TRACE_RUNNER_EXIT_AFTER_RUN    set to 1 to exit pane process after run
EOF_HINTS
}

print_observer_hints() {
  cat <<'EOF_HINTS'
Observer commands:
  curl -sS "$TRACE_API_BASE_URL/tasks" | jq .
  curl -sS -X POST "$TRACE_API_BASE_URL/benchmarks/evaluate" \
    -H 'content-type: application/json' \
    -d '{"report_id":"smoke_'"$(date +%s)"'"}' | jq .

  ls -la "$TRACE_ROOT/.trace/reports"
EOF_HINTS
}

iso_now() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

post_json() {
  local url="$1"
  local payload="$2"
  local attempts="${TRACE_RUNNER_RETRY_ATTEMPTS:-20}"
  local delay_sec="${TRACE_RUNNER_RETRY_DELAY_SEC:-1}"
  local attempt=1

  if ! [[ "$attempts" =~ ^[0-9]+$ ]] || (( attempts < 1 )); then
    echo "TRACE_RUNNER_RETRY_ATTEMPTS must be a positive integer" >&2
    exit 2
  fi
  if ! [[ "$delay_sec" =~ ^[0-9]+$ ]]; then
    echo "TRACE_RUNNER_RETRY_DELAY_SEC must be a non-negative integer" >&2
    exit 2
  fi

  while true; do
    if body="$(post_json_once "$url" "$payload")"; then
      printf '%s\n' "$body"
      return 0
    fi

    if (( attempt >= attempts )); then
      echo "request failed after $attempt attempts: POST $url" >&2
      return 1
    fi

    sleep "$delay_sec"
    attempt=$((attempt + 1))
  done
}

post_json_once() {
  local url="$1"
  local payload="$2"
  local response_file
  local http_code
  local body

  response_file="$(mktemp)"
  http_code="$(curl -sS -o "$response_file" -w '%{http_code}' -X POST "$url" -H 'content-type: application/json' -d "$payload")"
  body="$(cat "$response_file")"
  rm -f "$response_file"

  if (( http_code < 200 || http_code >= 300 )); then
    echo "request failed: POST $url ($http_code)" >&2
    echo "$body" >&2
    return 1
  fi

  printf '%s\n' "$body"
}

wait_for_api_ready() {
  local timeout_sec="${TRACE_RUNNER_READY_TIMEOUT_SEC:-30}"
  local sleep_sec=1
  local deadline

  if ! [[ "$timeout_sec" =~ ^[0-9]+$ ]] || (( timeout_sec < 1 )); then
    echo "TRACE_RUNNER_READY_TIMEOUT_SEC must be a positive integer" >&2
    exit 2
  fi

  deadline=$((SECONDS + timeout_sec))
  while (( SECONDS < deadline )); do
    if curl -sS --fail "$TRACE_API_BASE_URL/tasks" >/dev/null 2>&1; then
      return 0
    fi
    sleep "$sleep_sec"
  done

  echo "TRACE API did not become ready within ${timeout_sec}s: $TRACE_API_BASE_URL" >&2
  return 1
}

run_scripted_lane() {
  local task_count="${TRACE_RUNNER_TASK_COUNT:-1}"
  local task_prefix="${TRACE_RUNNER_TASK_PREFIX:-TASK-SMOKE}"
  local nonce="${TRACE_RUNNER_NONCE:-$(date +%s)}"
  local verdict="${TRACE_RUNNER_VERDICT:-pass}"

  if ! [[ "$task_count" =~ ^[0-9]+$ ]] || (( task_count < 1 )); then
    echo "TRACE_RUNNER_TASK_COUNT must be a positive integer" >&2
    exit 2
  fi

  echo "[TRACE RUNNER MODE]"
  echo "task_count=$task_count"
  echo "task_prefix=$task_prefix"
  echo "nonce=$nonce"
  echo "verdict=$verdict"
  echo

  wait_for_api_ready

  local index
  for ((index = 1; index <= task_count; index += 1)); do
    local task_id="${task_prefix}-${LANE_ID}-${nonce}-${index}"
    local run_id="${LANE_ID}-run-${nonce}-${index}"
    local candidate_id="C-${run_id}"
    local output_chunk="runner output lane=${LANE_ID} profile=${LANE_PROFILE} task=${task_id}"

    echo "[runner] task $index/$task_count task_id=$task_id run_id=$run_id"

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/claim" \
      "{\"worker_id\":\"$LANE_ID\",\"expected_epoch\":0,\"title\":\"$task_id\"}" \
      >/dev/null

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/runs/start" \
      "{\"run_id\":\"$run_id\",\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"profile\":\"$LANE_PROFILE\",\"provider\":\"openai\",\"model\":\"gpt-5-$LANE_PROFILE\"}" \
      >/dev/null

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/runs/$run_id/output" \
      "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"stream\":\"stdout\",\"encoding\":\"utf8\",\"chunk\":\"$output_chunk\",\"chunk_index\":0,\"final\":true}" \
      >/dev/null

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/runs/$run_id/candidates" \
      "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"candidate_id\":\"$candidate_id\"}" \
      >/dev/null

    post_json \
      "$TRACE_API_BASE_URL/events" \
      "{\"global_seq\":null,\"ts\":\"$(iso_now)\",\"task_id\":\"$task_id\",\"run_id\":\"$run_id\",\"kind\":\"verdict.recorded\",\"payload\":{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"verdict\":\"$verdict\"}}" \
      >/dev/null

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/release" \
      "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1}" \
      >/dev/null
  done

  echo
  echo "runner completed $task_count task(s)"
}

run_trace_server() {
  if command -v rustup >/dev/null 2>&1; then
    rustup run stable cargo run -p trace-server
    local rustup_exit=$?
    if [[ $rustup_exit -eq 0 ]]; then
      return 0
    fi
    if command -v cargo >/dev/null 2>&1; then
      echo "rustup stable invocation failed (exit $rustup_exit); falling back to 'cargo run -p trace-server'."
      cargo run -p trace-server
      return $?
    fi
    return $rustup_exit
  fi

  if command -v cargo >/dev/null 2>&1; then
    echo "rustup not found; falling back to 'cargo run -p trace-server'."
    cargo run -p trace-server
    return $?
  fi

  echo "Neither rustup nor cargo is available in PATH." >&2
  return 127
}

validate_lane_mode "$LANE_MODE"

case "$ROLE" in
  server)
    echo "[TRACE SERVER PANE]"
    print_common_banner
    echo
    echo "Starting trace-server. If it exits, shell stays open for debugging."
    set +e
    run_trace_server
    exit_code=$?
    set -e
    echo
    echo "trace-server exited with code $exit_code"
    exec "$SHELL_BIN" -i
    ;;
  observer)
    echo "[TRACE OBSERVER PANE]"
    print_common_banner
    echo
    print_observer_hints
    echo
    exec "$SHELL_BIN" -i
    ;;
  lane)
    echo "[TRACE LANE PANE]"
    print_common_banner
    echo

    if [[ "$LANE_MODE" == "runner" ]]; then
      print_runner_hints
      echo
      run_scripted_lane
      if [[ "${TRACE_RUNNER_EXIT_AFTER_RUN:-0}" == "1" ]]; then
        exit 0
      fi
      echo
      echo "runner completed; opening interactive shell"
    else
      print_lane_hints
    fi

    echo
    exec "$SHELL_BIN" -i
    ;;
  *)
    echo "unknown role: $ROLE" >&2
    exit 2
    ;;
esac
