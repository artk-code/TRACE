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
  TRACE_RUNNER_OUTPUT_MODE       codex | scripted (default: codex)
  TRACE_RUNNER_TASK_COUNT        number of tasks to emit (default: 1)
  TRACE_RUNNER_TASK_PREFIX       task prefix (default: TASK-SMOKE)
  TRACE_RUNNER_NONCE             run nonce (default: unix timestamp)
  TRACE_RUNNER_VERDICT           force verdict payload value (default: inferred)
  TRACE_RUNNER_OUTPUT_MAX_CHARS  truncate captured output to max chars (default: 16000)
  TRACE_RUNNER_CODEX_MODEL       optional codex model override
  TRACE_RUNNER_CODEX_PROFILE     optional codex config profile
  TRACE_RUNNER_CODEX_REASONING_EFFORT codex reasoning effort (default: low)
  TRACE_RUNNER_CODEX_SANDBOX     codex sandbox mode (default: read-only)
  TRACE_RUNNER_CODEX_PROMPT      optional extra instruction appended to task prompt
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

validate_runner_output_mode() {
  case "$1" in
    codex|scripted)
      ;;
    *)
      echo "TRACE_RUNNER_OUTPUT_MODE must be one of: codex, scripted" >&2
      exit 2
      ;;
  esac
}

validate_positive_integer() {
  local value="$1"
  local name="$2"
  if ! [[ "$value" =~ ^[0-9]+$ ]] || (( value < 1 )); then
    echo "$name must be a positive integer" >&2
    exit 2
  fi
}

base64_encode() {
  printf '%s' "$1" | base64 | tr -d '\n'
}

build_codex_prompt() {
  local task_id="$1"
  local run_id="$2"
  local index="$3"
  local task_count="$4"
  local extra_prompt="${TRACE_RUNNER_CODEX_PROMPT:-}"

  cat <<EOF_PROMPT
You are Codex lane ${LANE_ID} in TRACE runner mode.

Task metadata:
- task_id: ${task_id}
- run_id: ${run_id}
- lane_profile: ${LANE_PROFILE}
- task_index: ${index}/${task_count}

Return concise implementation-oriented output for this task:
1. one short plan (3 bullets max)
2. one compact code snippet
3. one verification note

Constraints:
- plain text or markdown only
- keep under 1200 words
- no surrounding commentary
${extra_prompt}
EOF_PROMPT
}

RUNNER_LAST_AGENT_EXIT_CODE=0
RUNNER_LAST_AGENT_OUTPUT=""

run_codex_task() {
  local task_id="$1"
  local run_id="$2"
  local index="$3"
  local task_count="$4"
  local output_file
  local stdout_file
  local stderr_file
  local prompt
  local sandbox_mode="${TRACE_RUNNER_CODEX_SANDBOX:-read-only}"
  local reasoning_effort="${TRACE_RUNNER_CODEX_REASONING_EFFORT:-low}"
  local -a codex_args

  output_file="$(mktemp)"
  stdout_file="$(mktemp)"
  stderr_file="$(mktemp)"
  prompt="$(build_codex_prompt "$task_id" "$run_id" "$index" "$task_count")"
  RUNNER_LAST_AGENT_OUTPUT=""

  codex_args=(exec --skip-git-repo-check --sandbox "$sandbox_mode" -C "$REPO_ROOT" -o "$output_file")
  if [[ -n "$reasoning_effort" ]]; then
    codex_args+=(-c "model_reasoning_effort='${reasoning_effort}'")
  fi
  if [[ -n "${TRACE_RUNNER_CODEX_PROFILE:-}" ]]; then
    codex_args+=(-p "$TRACE_RUNNER_CODEX_PROFILE")
  fi
  if [[ -n "${TRACE_RUNNER_CODEX_MODEL:-}" ]]; then
    codex_args+=(-m "$TRACE_RUNNER_CODEX_MODEL")
  fi
  codex_args+=("$prompt")

  if ! command -v codex >/dev/null 2>&1; then
    RUNNER_LAST_AGENT_EXIT_CODE=127
    RUNNER_LAST_AGENT_OUTPUT="[codex unavailable] lane=${LANE_ID} task=${task_id} run=${run_id}"
    rm -f "$output_file" "$stdout_file" "$stderr_file"
    return 0
  fi

  if codex "${codex_args[@]}" >"$stdout_file" 2>"$stderr_file"; then
    RUNNER_LAST_AGENT_EXIT_CODE=0
    local message
    message="$(cat "$output_file" 2>/dev/null || true)"
    if [[ -z "$message" ]]; then
      message="$(cat "$stdout_file" 2>/dev/null || true)"
    fi
    if [[ -z "$message" ]]; then
      message="[codex produced no output] lane=${LANE_ID} task=${task_id} run=${run_id}"
    fi
    RUNNER_LAST_AGENT_OUTPUT="$message"
  else
    RUNNER_LAST_AGENT_EXIT_CODE=$?
    local stderr_text
    local stdout_text
    stderr_text="$(cat "$stderr_file" 2>/dev/null || true)"
    stdout_text="$(cat "$stdout_file" 2>/dev/null || true)"
    RUNNER_LAST_AGENT_OUTPUT="$(printf '[codex exec failed] exit=%s lane=%s task=%s run=%s\nstderr:\n%s\nstdout:\n%s\n' \
      "$RUNNER_LAST_AGENT_EXIT_CODE" \
      "$LANE_ID" \
      "$task_id" \
      "$run_id" \
      "$stderr_text" \
      "$stdout_text")"
  fi

  rm -f "$output_file" "$stdout_file" "$stderr_file"
  return 0
}

run_scripted_lane() {
  local output_mode="${TRACE_RUNNER_OUTPUT_MODE:-codex}"
  local task_count="${TRACE_RUNNER_TASK_COUNT:-1}"
  local task_prefix="${TRACE_RUNNER_TASK_PREFIX:-TASK-SMOKE}"
  local nonce="${TRACE_RUNNER_NONCE:-$(date +%s)}"
  local forced_verdict="${TRACE_RUNNER_VERDICT:-}"
  local output_max_chars="${TRACE_RUNNER_OUTPUT_MAX_CHARS:-16000}"

  validate_runner_output_mode "$output_mode"
  validate_positive_integer "$task_count" "TRACE_RUNNER_TASK_COUNT"
  validate_positive_integer "$output_max_chars" "TRACE_RUNNER_OUTPUT_MAX_CHARS"

  echo "[TRACE RUNNER MODE]"
  echo "output_mode=$output_mode"
  echo "task_count=$task_count"
  echo "task_prefix=$task_prefix"
  echo "nonce=$nonce"
  if [[ -n "$forced_verdict" ]]; then
    echo "forced_verdict=$forced_verdict"
  fi
  echo

  wait_for_api_ready

  local index
  for ((index = 1; index <= task_count; index += 1)); do
    local task_id="${task_prefix}-${LANE_ID}-${nonce}-${index}"
    local run_id="${LANE_ID}-run-${nonce}-${index}"
    local candidate_id="C-${run_id}"
    local output_chunk
    local output_chunk_base64
    local verdict
    local model_name="${TRACE_RUNNER_CODEX_MODEL:-}"

    echo "[runner] task $index/$task_count task_id=$task_id run_id=$run_id"

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/claim" \
      "{\"worker_id\":\"$LANE_ID\",\"expected_epoch\":0,\"title\":\"$task_id\"}" \
      >/dev/null

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/runs/start" \
      "{\"run_id\":\"$run_id\",\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"profile\":\"$LANE_PROFILE\",\"provider\":\"openai\",\"model\":\"$model_name\"}" \
      >/dev/null

    if [[ "$output_mode" == "codex" ]]; then
      run_codex_task "$task_id" "$run_id" "$index" "$task_count"
      output_chunk="$RUNNER_LAST_AGENT_OUTPUT"
      if [[ -z "$model_name" ]]; then
        model_name="codex-auto-$LANE_PROFILE"
      fi
      if [[ -n "$forced_verdict" ]]; then
        verdict="$forced_verdict"
      elif (( RUNNER_LAST_AGENT_EXIT_CODE == 0 )); then
        verdict="pass"
      else
        verdict="fail"
      fi
    else
      output_chunk="runner output lane=${LANE_ID} profile=${LANE_PROFILE} task=${task_id}"
      if [[ -z "$model_name" ]]; then
        model_name="scripted-$LANE_PROFILE"
      fi
      verdict="${forced_verdict:-pass}"
    fi

    if (( ${#output_chunk} > output_max_chars )); then
      output_chunk="${output_chunk:0:output_max_chars}"
    fi
    output_chunk_base64="$(base64_encode "$output_chunk")"

    post_json \
      "$TRACE_API_BASE_URL/tasks/$task_id/runs/$run_id/output" \
      "{\"worker_id\":\"$LANE_ID\",\"lease_epoch\":1,\"stream\":\"stdout\",\"encoding\":\"base64\",\"chunk\":\"$output_chunk_base64\",\"chunk_index\":0,\"final\":true}" \
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
