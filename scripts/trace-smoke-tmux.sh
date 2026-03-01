#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LANE_BOOTSTRAP="$SCRIPT_DIR/trace-lane-shell.sh"

SESSION="${TRACE_TMUX_SESSION:-trace-smoke}"
TRACE_ROOT_VALUE="${TRACE_ROOT:-$REPO_ROOT/.trace-smoke}"
TRACE_SERVER_ADDR_VALUE="${TRACE_SERVER_ADDR:-127.0.0.1:18080}"
TRACE_RUNNER_OUTPUT_MODE_VALUE="${TRACE_RUNNER_OUTPUT_MODE:-}"
TRACE_RUNNER_TASK_COUNT_VALUE="${TRACE_RUNNER_TASK_COUNT:-}"
TRACE_RUNNER_TASK_PREFIX_VALUE="${TRACE_RUNNER_TASK_PREFIX:-}"
TRACE_RUNNER_REASONING_EFFORT_VALUE="${TRACE_RUNNER_CODEX_REASONING_EFFORT:-}"
TRACE_RUNNER_CODEX_PROMPT_VALUE="${TRACE_RUNNER_CODEX_PROMPT:-}"
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
  --runner-output-mode <mode>
                          TRACE_RUNNER_OUTPUT_MODE for spawned runner panes
  --runner-task-count <n>
                          TRACE_RUNNER_TASK_COUNT for spawned runner panes
  --runner-task-prefix <prefix>
                          TRACE_RUNNER_TASK_PREFIX for spawned runner panes
  --runner-reasoning-effort <value>
                          TRACE_RUNNER_CODEX_REASONING_EFFORT for spawned runner panes
  --runner-codex-prompt <text>
                          TRACE_RUNNER_CODEX_PROMPT for spawned runner panes

Commands:
  start [--no-attach]
      Create session with server + lanes (flash/high/extra) + observer.
  attach
      Attach to session.
  snapshot
      Emit machine-readable snapshot for windows/panes/config.
  status
      Show windows and panes for the session.
  send-keys <target> [--text <text>] [--key <key>] [--enter]
      Send read/write input to target pane (supports optional Enter).
  capture-pane <target> [lines]
      Capture read-only pane text (default lines: 200, max: 5000).
  validate-target <target>
      Validate that a tmux target exists in the session.
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
  $0 snapshot
  $0 send-keys trace-smoke:lanes.0 --text "echo hello" --enter
  $0 send-keys trace-smoke:lanes.0 --key C-c
  $0 capture-pane trace-smoke:lanes.0 300
  $0 validate-target trace-smoke:lanes
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
  local -a cmd

  if [[ "$mode" == "runner" ]]; then
    cmd=("env" "TRACE_RUNNER_EXIT_AFTER_RUN=1")
    if [[ -n "$TRACE_RUNNER_OUTPUT_MODE_VALUE" ]]; then
      cmd+=("TRACE_RUNNER_OUTPUT_MODE=$TRACE_RUNNER_OUTPUT_MODE_VALUE")
    fi
    if [[ -n "$TRACE_RUNNER_TASK_COUNT_VALUE" ]]; then
      cmd+=("TRACE_RUNNER_TASK_COUNT=$TRACE_RUNNER_TASK_COUNT_VALUE")
    fi
    if [[ -n "$TRACE_RUNNER_TASK_PREFIX_VALUE" ]]; then
      cmd+=("TRACE_RUNNER_TASK_PREFIX=$TRACE_RUNNER_TASK_PREFIX_VALUE")
    fi
    if [[ -n "$TRACE_RUNNER_REASONING_EFFORT_VALUE" ]]; then
      cmd+=("TRACE_RUNNER_CODEX_REASONING_EFFORT=$TRACE_RUNNER_REASONING_EFFORT_VALUE")
    fi
    if [[ -n "$TRACE_RUNNER_CODEX_PROMPT_VALUE" ]]; then
      cmd+=("TRACE_RUNNER_CODEX_PROMPT=$TRACE_RUNNER_CODEX_PROMPT_VALUE")
    fi
    cmd+=(
      "$LANE_BOOTSTRAP"
      "$lane"
      "$profile"
      "$REPO_ROOT"
      "$TRACE_ROOT_VALUE"
      "$TRACE_SERVER_ADDR_VALUE"
      "$role"
      "$mode"
    )
  else
    cmd=(
      "$LANE_BOOTSTRAP" \
      "$lane" \
      "$profile" \
      "$REPO_ROOT" \
      "$TRACE_ROOT_VALUE" \
      "$TRACE_SERVER_ADDR_VALUE" \
      "$role" \
      "$mode"
    )
  fi

  local quoted=""
  local arg
  for arg in "${cmd[@]}"; do
    local escaped
    escaped="$(printf "%q" "$arg")"
    if [[ -z "$quoted" ]]; then
      quoted="$escaped"
    else
      quoted="$quoted $escaped"
    fi
  done
  printf "%s" "$quoted"
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
      --runner-output-mode)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --runner-output-mode" >&2
          exit 2
        fi
        TRACE_RUNNER_OUTPUT_MODE_VALUE="$2"
        shift 2
        ;;
      --runner-task-count)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --runner-task-count" >&2
          exit 2
        fi
        TRACE_RUNNER_TASK_COUNT_VALUE="$2"
        shift 2
        ;;
      --runner-task-prefix)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --runner-task-prefix" >&2
          exit 2
        fi
        TRACE_RUNNER_TASK_PREFIX_VALUE="$2"
        shift 2
        ;;
      --runner-reasoning-effort)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --runner-reasoning-effort" >&2
          exit 2
        fi
        TRACE_RUNNER_REASONING_EFFORT_VALUE="$2"
        shift 2
        ;;
      --runner-codex-prompt)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --runner-codex-prompt" >&2
          exit 2
        fi
        TRACE_RUNNER_CODEX_PROMPT_VALUE="$2"
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
  if [[ -n "$TRACE_RUNNER_OUTPUT_MODE_VALUE" ]]; then
    tmux set-environment -t "$SESSION" TRACE_RUNNER_OUTPUT_MODE "$TRACE_RUNNER_OUTPUT_MODE_VALUE"
  fi
  if [[ -n "$TRACE_RUNNER_TASK_COUNT_VALUE" ]]; then
    tmux set-environment -t "$SESSION" TRACE_RUNNER_TASK_COUNT "$TRACE_RUNNER_TASK_COUNT_VALUE"
  fi
  if [[ -n "$TRACE_RUNNER_TASK_PREFIX_VALUE" ]]; then
    tmux set-environment -t "$SESSION" TRACE_RUNNER_TASK_PREFIX "$TRACE_RUNNER_TASK_PREFIX_VALUE"
  fi
  if [[ -n "$TRACE_RUNNER_REASONING_EFFORT_VALUE" ]]; then
    tmux set-environment -t "$SESSION" TRACE_RUNNER_CODEX_REASONING_EFFORT "$TRACE_RUNNER_REASONING_EFFORT_VALUE"
  fi
  if [[ -n "$TRACE_RUNNER_CODEX_PROMPT_VALUE" ]]; then
    tmux set-environment -t "$SESSION" TRACE_RUNNER_CODEX_PROMPT "$TRACE_RUNNER_CODEX_PROMPT_VALUE"
  fi

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
  tmux list-panes -a -f "#{==:#{session_name},$SESSION}" -F "#{session_name}:#{window_name}.#{pane_index} lane=#{@trace_lane_name} mode=#{@trace_lane_mode} title=#{pane_title} dead=#{pane_dead} dead_status=#{pane_dead_status} pid=#{pane_pid} cmd=#{pane_current_command}"
  echo
  echo "session config:"
  echo "TRACE_ROOT=$(session_env_value TRACE_ROOT || echo "<unset>")"
  echo "TRACE_SERVER_ADDR=$(session_env_value TRACE_SERVER_ADDR || echo "<unset>")"
  echo "TRACE_RUNNER_OUTPUT_MODE=$(session_env_value TRACE_RUNNER_OUTPUT_MODE || echo "<unset>")"
  echo "TRACE_RUNNER_TASK_COUNT=$(session_env_value TRACE_RUNNER_TASK_COUNT || echo "<unset>")"
  echo "TRACE_RUNNER_TASK_PREFIX=$(session_env_value TRACE_RUNNER_TASK_PREFIX || echo "<unset>")"
  echo "TRACE_RUNNER_CODEX_REASONING_EFFORT=$(session_env_value TRACE_RUNNER_CODEX_REASONING_EFFORT || echo "<unset>")"
}

snapshot_session() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi
  hydrate_config_from_session

  printf "session\t%s\n" "$SESSION"
  printf "config\tTRACE_ROOT\t%s\n" "$(session_env_value TRACE_ROOT || true)"
  printf "config\tTRACE_SERVER_ADDR\t%s\n" "$(session_env_value TRACE_SERVER_ADDR || true)"
  printf "config\tTRACE_RUNNER_OUTPUT_MODE\t%s\n" "$(session_env_value TRACE_RUNNER_OUTPUT_MODE || true)"
  printf "config\tTRACE_RUNNER_TASK_COUNT\t%s\n" "$(session_env_value TRACE_RUNNER_TASK_COUNT || true)"
  printf "config\tTRACE_RUNNER_TASK_PREFIX\t%s\n" "$(session_env_value TRACE_RUNNER_TASK_PREFIX || true)"
  printf "config\tTRACE_RUNNER_CODEX_REASONING_EFFORT\t%s\n" "$(session_env_value TRACE_RUNNER_CODEX_REASONING_EFFORT || true)"

  tmux list-windows -t "$SESSION" -F "window\t#{window_index}\t#{window_name}\t#{window_id}\t#{window_active}" \
    | sed $'s/\\\\t/\t/g'
  tmux list-panes -a -f "#{==:#{session_name},$SESSION}" -F "pane\t#{pane_id}\t#{session_name}\t#{window_index}\t#{window_name}\t#{pane_index}\t#{pane_title}\t#{@trace_lane_name}\t#{@trace_lane_mode}\t#{pane_active}\t#{pane_dead}\t#{pane_dead_status}\t#{pane_pid}\t#{pane_current_command}" \
    | sed $'s/\\\\t/\t/g'
}

capture_pane_output() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi

  local target="${1:-}"
  local lines="${2:-200}"
  if [[ -z "$target" ]]; then
    echo "usage: $0 [global opts] capture-pane <target> [lines]" >&2
    exit 2
  fi
  if ! [[ "$lines" =~ ^[0-9]+$ ]] || (( lines < 1 || lines > 5000 )); then
    echo "lines must be an integer between 1 and 5000" >&2
    exit 2
  fi
  if ! tmux list-panes -t "$target" >/dev/null 2>&1; then
    echo "target '$target' does not exist in session '$SESSION'" >&2
    exit 1
  fi

  tmux capture-pane -p -t "$target" -S "-$lines"
}

send_keys() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi

  local target="${1:-}"
  shift || true
  if [[ -z "$target" ]]; then
    echo "usage: $0 [global opts] send-keys <target> [--text <text>] [--key <key>] [--enter]" >&2
    exit 2
  fi
  if ! tmux list-panes -t "$target" >/dev/null 2>&1; then
    echo "target '$target' does not exist in session '$SESSION'" >&2
    exit 1
  fi

  local text=""
  local key=""
  local press_enter=0

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --text)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --text" >&2
          exit 2
        fi
        text="$2"
        shift 2
        ;;
      --key)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --key" >&2
          exit 2
        fi
        key="$2"
        shift 2
        ;;
      --enter)
        press_enter=1
        shift
        ;;
      *)
        echo "unknown send-keys option: $1" >&2
        exit 2
        ;;
    esac
  done

  if [[ -z "$text" && -z "$key" && "$press_enter" -eq 0 ]]; then
    echo "send-keys requires --text and/or --key and/or --enter" >&2
    exit 2
  fi

  if [[ -n "$text" ]]; then
    tmux send-keys -t "$target" -l "$text"
  fi
  if [[ -n "$key" ]]; then
    tmux send-keys -t "$target" "$key"
  fi
  if [[ "$press_enter" -eq 1 ]]; then
    tmux send-keys -t "$target" Enter
  fi

  echo "sent keys to '$target'"
}

validate_target() {
  require_tmux
  if ! session_exists; then
    echo "session '$SESSION' does not exist." >&2
    exit 1
  fi

  local target="${1:-}"
  if [[ -z "$target" ]]; then
    echo "usage: $0 [global opts] validate-target <target>" >&2
    exit 2
  fi

  if ! tmux list-panes -t "$target" >/dev/null 2>&1; then
    echo "target '$target' does not exist in session '$SESSION'" >&2
    exit 1
  fi

  echo "target '$target' exists"
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
      tmux list-panes -a -f "#{==:#{session_name},$SESSION}" -F "#{pane_id}|#{@trace_lane_name}|#{pane_title}|#{pane_dead}|#{pane_dead_status}" \
        | awk -F '|' -v target_lane="$lane_name" -v target_title="$lane_title" '$2==target_lane || $3==target_title {print; exit}'
    )"

    if [[ -n "$lane_row" ]]; then
      local pane_id pane_lane pane_title pane_dead pane_dead_status
      IFS='|' read -r pane_id pane_lane pane_title pane_dead pane_dead_status <<<"$lane_row"
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
  snapshot)
    snapshot_session
    ;;
  status)
    status_session
    ;;
  send-keys)
    send_keys "$@"
    ;;
  capture-pane)
    capture_pane_output "$@"
    ;;
  validate-target)
    validate_target "$@"
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
