# TRACE JJ Multi-Agent Patch Workflow

Date: 2026-03-01  
Status: active bootstrap

This guide defines the initial `jj`-based workflow for parallel agent patch lanes.

## Goal
Run multiple agents in parallel workspaces, then export/publish each lane as an isolated patch for review/integration.

## Prerequisites
- `jj` installed.
- TRACE repo cloned with Git remote configured.
- Typical TRACE build tooling already installed (`rust`, `pnpm`, etc.).

Install `jj`:
- macOS: `brew install jj`
- Linux: `cargo install --locked jj-cli`

## One-Time Repo Bootstrap
From repo root:

```bash
scripts/trace-jj.sh bootstrap
```

This:
- verifies `jj` is available
- initializes `jj` repo if missing (`jj git init`)
- fetches/tracks default remote bookmarks when possible

Configure commit identity (required before publishing):

```bash
jj config set --user user.name "Your Name"
jj config set --user user.email "you@example.com"
```

## Create Agent Lanes (Workspaces)
Create one workspace per agent:

```bash
scripts/trace-jj.sh lane-add codex-a
scripts/trace-jj.sh lane-add codex-b
scripts/trace-jj.sh lane-add codex-c
```

Defaults:
- base revision: `trunk()`
- workspace root: `.workspaces/<lane_name>`

List lanes:

```bash
scripts/trace-jj.sh lane-list
```

## Agent Workflow In Each Lane
Inside a lane workspace (`cd .workspaces/codex-a`):

1. Make code changes.
2. Commit lane patch:
```bash
jj commit -m "agent(codex-a): implement <slice>"
```

Note:
- after `jj commit`, the completed patch is usually `@-` (current `@` is new working-copy change).

## Export Lane Patch Artifacts
Generate a patch file for handoff:

```bash
scripts/trace-jj.sh patch /tmp/codex-a.patch @-
```

## Publish Lane Patch To Remote
Publish a lane revision as a bookmark branch:

```bash
scripts/trace-jj.sh publish agent/codex-a/feature-x @- origin
```

This runs:
- `jj bookmark set <bookmark> -r <revset>`
- `jj git push --remote <remote> --bookmark <bookmark>`

## Integrate Good Lanes And Drop Bad Lanes
When multiple agents propose competing changes, compose only the winning revisions:

```bash
scripts/trace-jj.sh integrate \
  --base trunk() \
  --good good-a \
  --good good-b \
  --bad bad-a \
  --message "feat: integrate selected agent revisions"
```

What this does:
- creates a new integration change on `--base`
- squashes each `--good` revision into the integration change
- abandons each `--bad` revision (optional)
- keeps a single integration message for the composed change

Then publish the integration result:

```bash
scripts/trace-jj.sh publish agent/integration/selected @ origin
```

## Browser + API Controls
TRACE web UI now exposes a **JJ Workflow** panel that calls server-side orchestration routes:

- `POST /orchestrator/jj/bootstrap`
- `POST /orchestrator/jj/status`
- `POST /orchestrator/jj/lane-add`
- `POST /orchestrator/jj/lane-list`
- `POST /orchestrator/jj/lane-forget`
- `POST /orchestrator/jj/lane-root`
- `POST /orchestrator/jj/patch`
- `POST /orchestrator/jj/publish`
- `POST /orchestrator/jj/integrate`

The server executes `scripts/trace-jj.sh` for these actions (override path with `TRACE_JJ_ORCH_SCRIPT`).

## Lane Cleanup
Forget a lane workspace in jj metadata:

```bash
scripts/trace-jj.sh lane-forget codex-a
```

Note:
- this does not delete files from disk.

## Current Scope
This is the bootstrap phase for the multi-agent patch workflow. It intentionally does not yet automate:
- scoring lane patches
- selecting winner patches
- stacked merge generation
- PR creation policy/gates

Those are next-phase tasks after deterministic eval and merge strategy are finalized.
