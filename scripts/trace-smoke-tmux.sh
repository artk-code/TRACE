#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LANE_BOOTSTRAP="$SCRIPT_DIR/trace-lane-shell.sh"

SESSION="${TRACE_TMUX_SESSION:-trace-smoke}"
TRACE_ROOT_VALUE="${TRACE_ROOT:-$REPO_ROOT/.trace-smoke}"
TRACE_SERVER_ADDR_VALUE="${TRACE_SERVER_ADDR:-127.0.0.1:18080}"
TRACE_ROOT_EXPLICIT=0
TRACE_ADDR_EXPLICIT=0

usage() {
  cat <<EOF
TRACE tmux smoke orchestrator

Usage:
  $0 [global options] <command> [command args]

Global options (must appear before command):
  --session <name>       tmux session name (default: $SESSION)
  --trace-root <path>    TRACE_ROOT for all panes (default: $TRACE_ROOT_VALUE)
  --addr <host:port>     TRACE_SERVER_ADDR (default: $TRACE_SERVER_ADDR_VALUE)

Commands:
  start [--no-attach]
      Create session with server + lanes (flash/high/extra) + observer.
  attach
      Attach to session.
  status
      Show windows and panes for the session.
  add-lane <lane_name> [profile] [mode]
      Add new lane window.
      mode: interactive | runner (default: interactive)
  add-pane <lane_name> [profile] [target] [mode]
      Split target window/pane and start lane.
      default target: <session>:lanes
      mode: interactive | runner (default: interactive)
  wait-lane <lane_name> [timeout_sec]
      Wait for a lane pane to finish (runner mode).
  stop
      Kill tmux session.
  help
      Show this message.

Examples:
  $0 start
  $0 --session trace-smoke attach
  $0 add-lane codex4 high
  $0 add-lane codex4 high runner
  $0 add-pane codex5 flash trace-smoke:lanes runner
  $0 wait-lane codex5 180
EOF
}

require_tmux() {
  if ! command -v tmux >/dev/null 2>&1; then
    echo "tmux is required but not found in PATH." >&2
    exit 1
  fi
}

session_exists() {
  tmux has-session -t "$SESSION" 2>/dev/null
}

is_lane_mode() {
  case "$1" in
    interactive|runner)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

validate_lane_mode() {
  if ! is_lane_mode "$1"; then
    echo "invalid lane mode: $1 (allowed: interactive, runner)" >&2
    exit 2
  fi
}

build_lane_cmd() {
  local role="$1"
  local lane="$2"
  local profile="$3"
  local mode="${4:-interactive}"
  if [[ "$mode" == "runner" ]]; then
    printf "%q %q %q %q %q %q %q %q %q %q" \
      "env" \
      "TRACE_RUNNER_EXIT_AFTER_RUN=1" \
      "$LANE_BOOTSTRAP" \
      "$lane" \
      "$profile" \
      "$REPO_ROOT" \
      "$TRACE_ROOT_VALUE" \
      "$TRACE_SERVER_ADDR_VALUE" \
      "$role" \
      "$mode"
  else
    printf "%q %q %q %q %q %q %q %q" \
      "$LANE_BOOTSTRAP" \
      "$lane" \
      "$profile" \
      "$REPO_ROOT" \
      "$TRACE_ROOT_VALUE" \
      "$TRACE_SERVER_ADDR_VALUE" \
      "$role" \
      "$mode"
  fi
}

configure_lane_pane() {
  local pane_id="$1"
  local lane_name="$2"
  local mode="$3"

  tmux select-pane -t "$pane_id" -T "lane-${lane_name}"
  tmux set-option -pt "$pane_id" "@trace_lane_name" "$lane_name" >/dev/null
  tmux set-option -pt "$pane_id" "@trace_lane_mode" "$mode" >/dev/null
  if [[ "$mode" == "runner" ]]; then
    tmux set-window-option -t "$pane_id" remain-on-exit on >/dev/null
  fi
}

parse_global_options() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --session)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --session" >&2
          exit 2
        fi
        SESSION="$2"
        shift 2
        ;;
      --trace-root)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --trace-root" >&2
          exit 2
        fi
        TRACE_ROOT_VALUE="$2"
        TRACE_ROOT_EXPLICIT=1
        shift 2
        ;;
      --addr)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --addr" >&2
          exit 2
        fi
        TRACE_SERVER_ADDR_VALUE="$2"
        TRACE_ADDR_EXPLICIT=1
        shift 2
        ;;
      *)
        break
        ;;
    esac
  done

  REM_ARGS=("$@")
}

session_env_value() {
  local key="$1"
  local line
  if ! line="$(tmux show-environment -t "$SESSION" "$key" 2>/dev/null)"; then
    return 1
  fi
  if [[ "$line" == -* ]]; then
    return 1
  fi
  printf "%s\n" "${line#*=}"
}

hydrate_config_from_session() {
  if ! session_exists; then
    return 0
  fi

  if [[ "$TRACE_ROOT_EXPLICIT" -eq 0 ]]; then
    local existing_root=""
    existing_root="$(session_env_value TRACE_ROOT || true)"
    if [[ -n "$existing_root" ]]; then
      TRACE_ROOT_VALUE="$existing_root"
    fi
  fi

  if [[ "$TRACE_ADDR_EXPLICIT" -eq 0 ]]; then
    local existing_addr=""
    existing_addr="$(session_env_value TRACE_SERVER_ADDR || true)"
    if [[ -n "$existing_addr" ]]; then
      TRACE_SERVER_ADDR_VALUE="$existing_addr"
    fi
  fi
}

start_session() {
  local attach=1
  if [[ ${1:-} == "--no-attach" ]]; then
    attach=0
  elif [[ $# -gt 0 ]]; then
    echo "unknown start argument: $1" >&2
    exit 2
  fi

  require_tmux
  mkdir -p "$TRACE_ROOT_VALUE"

  if session_exists; then
    echo "session '$SESSION' already exists. use '$0 --session $SESSION attach'" >&2
    exit 1
  fi

  tmux new-session -d -s "$SESSION" -n server "$(build_lane_cmd server server control interactive)"
  tmux set-environment -t "$SESSION" TRACE_ROOT "$TRACE_ROOT_VALUE"
  tmux set-environment -t "$SESSION" TRACE_SERVER_ADDR "$TRACE_SERVER_ADDR_VALUE"
  tmux set-environment -t "$SESSION" TRACE_API_BASE_URL "http://$TRACE_SERVER_ADDR_VALUE"
  tmux set-environment -t "$SESSION" TRACE_REPO_ROOT "$REPO_ROOT"

  tmux new-window -t "${SESSION}:" -n lanes "$(build_lane_cmd lane flash flash interactive)"
  tmux split-window -t "${SESSION}:lanes" -h "$(build_lane_cmd lane high high interactive)"
  tmux split-window -t "${SESSION}:lanes" -v "$(build_lane_cmd lane extra extra interactive)"
  tmux select-layout -t "${SESSION}:lanes" tiled

  tmux new-window -t "${SESSION}:" -n observer "$(build_lane_cmd observer observer observer interactive)"
  tmux select-window -t "${SESSION}:lanes"

  echo "started session '$SESSION'"
  echo "TRACE_ROOT=$TRACE_ROOT_VALUE"
  echo "TRACE_SERVER_ADDR=$TRACE_SERVER_ADDR_VALUE"
  echo "attach with: $0 --session $SESSION attach"

  if [[ $attach -eq 1 ]]; then
    tmux attach -t "$SESSION"
  fi
}

attach_session() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi
  tmux attach -t "$SESSION"
}

status_session() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi
  hydrate_config_from_session

  echo "windows:"
  tmux list-windows -t "$SESSION"
  echo
  echo "panes:"
  tmux list-panes -a -f "#{==:#{session_name},$SESSION}" -F "#{session_name}:#{window_name}.#{pane_index} title=#{pane_title} dead=#{pane_dead} dead_status=#{pane_dead_status} pid=#{pane_pid} cmd=#{pane_current_command}"
  echo
  echo "session config:"
  echo "TRACE_ROOT=$(session_env_value TRACE_ROOT || echo "<unset>")"
  echo "TRACE_SERVER_ADDR=$(session_env_value TRACE_SERVER_ADDR || echo "<unset>")"
}

add_lane() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi
  hydrate_config_from_session

  local lane_name="${1:-}"
  local profile="${2:-}"
  local mode="${3:-interactive}"
  if [[ -z "$lane_name" ]]; then
    echo "usage: $0 [global opts] add-lane <lane_name> [profile] [mode]" >&2
    exit 2
  fi
  if [[ -z "$profile" ]]; then
    profile="$lane_name"
  fi
  validate_lane_mode "$mode"

  local pane_id
  pane_id="$(tmux new-window -P -F "#{pane_id}" -t "${SESSION}:" -n "lane-${lane_name}" "$(build_lane_cmd lane "$lane_name" "$profile" "$mode")")"
  configure_lane_pane "$pane_id" "$lane_name" "$mode"
  echo "added lane window: lane-${lane_name} (profile=$profile mode=$mode)"
}

add_pane() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi
  hydrate_config_from_session

  local lane_name="${1:-}"
  local profile="${2:-}"
  local target="${SESSION}:lanes"
  local mode="interactive"
  local arg3="${3:-}"
  local arg4="${4:-}"
  if [[ -z "$lane_name" ]]; then
    echo "usage: $0 [global opts] add-pane <lane_name> [profile] [target] [mode]" >&2
    exit 2
  fi
  if [[ -z "$profile" ]]; then
    profile="$lane_name"
  fi
  if [[ -n "$arg3" ]]; then
    if is_lane_mode "$arg3" && [[ -z "$arg4" ]]; then
      mode="$arg3"
    else
      target="$arg3"
      if [[ -n "$arg4" ]]; then
        mode="$arg4"
      fi
    fi
  fi
  validate_lane_mode "$mode"

  local pane_id
  pane_id="$(tmux split-window -P -F "#{pane_id}" -t "$target" -v "$(build_lane_cmd lane "$lane_name" "$profile" "$mode")")"
  configure_lane_pane "$pane_id" "$lane_name" "$mode"
  tmux select-layout -t "$target" tiled || true
  echo "added lane pane: $lane_name (profile=$profile mode=$mode) on target $target"
}

wait_lane() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi

  local lane_name="${1:-}"
  local timeout_sec="${2:-180}"
  local lane_title="lane-${lane_name}"
  local deadline

  if [[ -z "$lane_name" ]]; then
    echo "usage: $0 [global opts] wait-lane <lane_name> [timeout_sec]" >&2
    exit 2
  fi
  if ! [[ "$timeout_sec" =~ ^[0-9]+$ ]] || (( timeout_sec < 1 )); then
    echo "timeout_sec must be a positive integer" >&2
    exit 2
  fi

  deadline=$((SECONDS + timeout_sec))
  while (( SECONDS <= deadline )); do
    local lane_row=""
    lane_row="$(
      tmux list-panes -a -f "#{==:#{session_name},$SESSION}" -F "#{pane_id}\t#{pane_title}\t#{pane_dead}\t#{pane_dead_status}" \
        | awk -F '\t' -v target_title="$lane_title" '$2==target_title {print; exit}'
    )"

    if [[ -n "$lane_row" ]]; then
      local pane_id pane_title pane_dead pane_dead_status
      IFS=$'\t' read -r pane_id pane_title pane_dead pane_dead_status <<<"$lane_row"
      if [[ "$pane_dead" == "1" ]]; then
        if [[ "$pane_dead_status" == "0" ]]; then
          echo "lane '$lane_name' completed successfully"
          return 0
        fi
        echo "lane '$lane_name' failed with exit status $pane_dead_status" >&2
        return 1
      fi
    fi

    sleep 1
  done

  echo "timed out waiting for lane '$lane_name' after ${timeout_sec}s" >&2
  return 1
}

stop_session() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi
  tmux kill-session -t "$SESSION"
  echo "stopped session '$SESSION'"
}

parse_global_options "$@"
set -- "${REM_ARGS[@]}"

command="${1:-help}"
shift || true

case "$command" in
  start)
    start_session "$@"
    ;;
  attach)
    attach_session
    ;;
  status)
    status_session
    ;;
  add-lane)
    add_lane "$@"
    ;;
  add-pane)
    add_pane "$@"
    ;;
  wait-lane)
    wait_lane "$@"
    ;;
  stop)
    stop_session
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    echo "unknown command: $command" >&2
    usage >&2
    exit 2
    ;;
esac
