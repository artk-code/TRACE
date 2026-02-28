#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 6 ]]; then
  echo "usage: $0 <lane_id> <profile> <repo_root> <trace_root> <trace_server_addr> <role>" >&2
  echo "roles: lane | observer | server" >&2
  exit 2
fi

LANE_ID="$1"
PROFILE="$2"
REPO_ROOT="$3"
TRACE_ROOT="$4"
TRACE_SERVER_ADDR="$5"
ROLE="$6"

TRACE_API_BASE_URL="${TRACE_API_BASE_URL:-http://${TRACE_SERVER_ADDR}}"
SHELL_BIN="${SHELL:-/bin/zsh}"

export TRACE_ROOT TRACE_SERVER_ADDR TRACE_API_BASE_URL
export LANE_ID
export LANE_PROFILE="$PROFILE"

cd "$REPO_ROOT"

print_common_banner() {
  echo "TRACE_ROOT=$TRACE_ROOT"
  echo "TRACE_SERVER_ADDR=$TRACE_SERVER_ADDR"
  echo "TRACE_API_BASE_URL=$TRACE_API_BASE_URL"
  echo "LANE_ID=$LANE_ID"
  echo "LANE_PROFILE=$LANE_PROFILE"
}

print_lane_hints() {
  cat <<'EOF'
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
EOF
}

print_observer_hints() {
  cat <<'EOF'
Observer commands:
  curl -sS "$TRACE_API_BASE_URL/tasks" | jq .
  curl -sS -X POST "$TRACE_API_BASE_URL/benchmarks/evaluate" \
    -H 'content-type: application/json' \
    -d '{"report_id":"smoke_'"$(date +%s)"'"}' | jq .

  ls -la "$TRACE_ROOT/.trace/reports"
EOF
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
    print_lane_hints
    echo
    exec "$SHELL_BIN" -i
    ;;
  *)
    echo "unknown role: $ROLE" >&2
    exit 2
    ;;
esac
