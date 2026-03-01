#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEFAULT_REMOTE="${TRACE_JJ_REMOTE:-origin}"
DEFAULT_LANE_DIR="${TRACE_JJ_LANE_DIR:-$REPO_ROOT/.workspaces}"

usage() {
  cat <<EOF
TRACE jj multi-agent workflow helper

Usage:
  $0 <command> [args]

Commands:
  bootstrap [remote]
      Ensure jj is installed and repo is initialized.
      Optionally fetch/track trunk bookmarks from remote (default: $DEFAULT_REMOTE).

  status
      Show jj status, workspaces, and recent log from trunk()..@.

  lane-add <lane_name> [base_revset] [destination]
      Create a dedicated workspace for an agent lane.
      base_revset default: trunk()
      destination default: $DEFAULT_LANE_DIR/<lane_name>

  lane-list
      List known jj workspaces.

  lane-forget <lane_name>
      Forget workspace metadata (does not delete files on disk).

  lane-root <lane_name>
      Print workspace root path for lane.

  patch <output_path> [revset]
      Export a git-format patch for a revision (default: @-).

  publish <bookmark> [revset] [remote]
      Set bookmark to revision (default: @-) and push it (default remote: $DEFAULT_REMOTE).

  help
      Show this message.

Examples:
  $0 bootstrap
  $0 lane-add codex-a
  $0 lane-add codex-b 'trunk()'
  $0 status
  $0 patch /tmp/codex-a.patch @-
  $0 publish agent/codex-a/feature @- origin
EOF
}

require_jj() {
  if ! command -v jj >/dev/null 2>&1; then
    cat <<EOF >&2
jj is not installed.

Install options:
  macOS: brew install jj
  Linux: cargo install --locked jj-cli
EOF
    exit 1
  fi
}

ensure_jj_repo() {
  if jj root >/dev/null 2>&1; then
    return 0
  fi

  if [[ -d "$REPO_ROOT/.git" ]]; then
    echo "initializing jj in existing git repo: $REPO_ROOT"
    (cd "$REPO_ROOT" && jj git init >/dev/null)
    return 0
  fi

  echo "no git repo found at $REPO_ROOT; cannot initialize jj" >&2
  exit 1
}

has_remote() {
  local remote="$1"
  jj git remote list --color never | awk '{print $1}' | grep -qx "$remote"
}

identity_hint() {
  local name
  local email
  name="$(jj config get user.name || true)"
  email="$(jj config get user.email || true)"
  if [[ -z "${name// }" || -z "${email// }" ]]; then
    cat <<EOF
warning: jj user identity is not configured. set before publishing:
  jj config set --user user.name "Your Name"
  jj config set --user user.email "you@example.com"
EOF
  fi
}

bootstrap() {
  local remote="${1:-$DEFAULT_REMOTE}"
  require_jj
  ensure_jj_repo

  if has_remote "$remote"; then
    jj git fetch --remote "$remote" >/dev/null || true
    jj bookmark track main --remote "$remote" >/dev/null 2>&1 || true
    jj bookmark track trunk --remote "$remote" >/dev/null 2>&1 || true
  fi
  identity_hint

  cat <<EOF
jj ready in: $(jj root)
remote: $remote

next:
  $0 lane-add codex-a
  cd $DEFAULT_LANE_DIR/codex-a
  jj st
EOF
}

status_cmd() {
  require_jj
  ensure_jj_repo
  jj st
  echo
  echo "workspaces:"
  jj workspace list
  echo
  echo "recent trunk()..@ history:"
  jj log -r "trunk()..@" -n 12
}

lane_add() {
  require_jj
  ensure_jj_repo
  local lane_name="${1:-}"
  local base_revset="${2:-trunk()}"
  local destination="${3:-$DEFAULT_LANE_DIR/$lane_name}"

  if [[ -z "$lane_name" ]]; then
    echo "usage: $0 lane-add <lane_name> [base_revset] [destination]" >&2
    exit 2
  fi

  if [[ -e "$destination" ]]; then
    echo "destination already exists: $destination" >&2
    exit 1
  fi

  mkdir -p "$(dirname "$destination")"
  jj workspace add \
    --name "$lane_name" \
    -r "$base_revset" \
    -m "agent($lane_name): start lane" \
    "$destination"

  cat <<EOF
lane workspace created:
  name: $lane_name
  root: $destination

next:
  cd $destination
  jj st
EOF
}

lane_list() {
  require_jj
  ensure_jj_repo
  jj workspace list
}

lane_forget() {
  require_jj
  ensure_jj_repo
  local lane_name="${1:-}"
  if [[ -z "$lane_name" ]]; then
    echo "usage: $0 lane-forget <lane_name>" >&2
    exit 2
  fi
  jj workspace forget "$lane_name"
}

lane_root() {
  require_jj
  ensure_jj_repo
  local lane_name="${1:-}"
  if [[ -z "$lane_name" ]]; then
    echo "usage: $0 lane-root <lane_name>" >&2
    exit 2
  fi
  jj workspace root --name "$lane_name"
}

patch_cmd() {
  require_jj
  ensure_jj_repo
  local output_path="${1:-}"
  local revset="${2:-@-}"

  if [[ -z "$output_path" ]]; then
    echo "usage: $0 patch <output_path> [revset]" >&2
    exit 2
  fi

  mkdir -p "$(dirname "$output_path")"
  jj diff --git --color never -r "$revset" >"$output_path"
  echo "wrote patch: $output_path (revset=$revset)"
}

publish_cmd() {
  require_jj
  ensure_jj_repo
  local bookmark="${1:-}"
  local revset="${2:-@-}"
  local remote="${3:-$DEFAULT_REMOTE}"

  if [[ -z "$bookmark" ]]; then
    echo "usage: $0 publish <bookmark> [revset] [remote]" >&2
    exit 2
  fi

  if ! has_remote "$remote"; then
    echo "remote not found: $remote" >&2
    echo "available remotes:" >&2
    jj git remote list --color never >&2
    exit 1
  fi

  jj bookmark set "$bookmark" -r "$revset"
  jj git push --remote "$remote" --bookmark "$bookmark"
  echo "published bookmark '$bookmark' at revset '$revset' to remote '$remote'"
}

command_name="${1:-help}"
shift || true

case "$command_name" in
  bootstrap)
    bootstrap "$@"
    ;;
  status)
    status_cmd
    ;;
  lane-add)
    lane_add "$@"
    ;;
  lane-list)
    lane_list
    ;;
  lane-forget)
    lane_forget "$@"
    ;;
  lane-root)
    lane_root "$@"
    ;;
  patch)
    patch_cmd "$@"
    ;;
  publish)
    publish_cmd "$@"
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    echo "unknown command: $command_name" >&2
    usage >&2
    exit 2
    ;;
esac
