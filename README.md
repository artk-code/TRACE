# TRACE
TRACE is a *local-first* harness that binds agent work to tasks, records immutable traces, versions outputs as ChangeSets, evaluates candidates deterministically, and supports recombination/stacking to pick a winner.

## Active Planning Docs
- [AGENTS.md](/Users/artk/Documents/GitHub/TRACE/AGENTS.md)
- [BUILD_SEQUENCE_PLAN_v2.md](/Users/artk/Documents/GitHub/TRACE/BUILD_SEQUENCE_PLAN_v2.md)
- [SMOKETEST_EVAL_PLAN_v1.md](/Users/artk/Documents/GitHub/TRACE/SMOKETEST_EVAL_PLAN_v1.md)
- [FRONTEND_PLAN_v4.md](/Users/artk/Documents/GitHub/TRACE/FRONTEND_PLAN_v4.md)
- [FRONTEND_PLAN_v3.md](/Users/artk/Documents/GitHub/TRACE/FRONTEND_PLAN_v3.md)

## Archived Planning Docs
- [archive/phase0_docs](/Users/artk/Documents/GitHub/TRACE/archive/phase0_docs)

## Workspace Layout
- Rust workspace crates live in `/Users/artk/Documents/GitHub/TRACE/crates`.
- Frontend package lives in `/Users/artk/Documents/GitHub/TRACE/web`.
- Canonical event log path is `.trace/events/events.jsonl`.

## Ubuntu LTS Build Guide (22.04/24.04)
1. Install OS packages:
```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev curl git tmux jq ca-certificates
```
2. Install Rust toolchain (`rustup` + stable):
```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
```
3. Install Node.js 20 LTS + pnpm:
```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
corepack enable
corepack prepare pnpm@9 --activate
```
4. Install workspace dependencies:
```bash
cd /Users/artk/Documents/GitHub/TRACE
pnpm install
```

## Build + Test
1. Rust workspace regression:
```bash
rustup run stable cargo test --workspace
```
2. Web regression:
```bash
pnpm --dir web test
pnpm --dir web build
```

## Local Run (Server + Web)
1. Start TRACE server:
```bash
TRACE_SERVER_ADDR=127.0.0.1:18086 TRACE_ROOT=/tmp/trace-web-smoke cargo run -p trace-server
```
2. In another terminal, run web UI:
```bash
VITE_TRACE_API_BASE_URL=http://127.0.0.1:18086 pnpm --dir web dev --host 127.0.0.1 --port 4173
```
3. Open `http://127.0.0.1:4173` and use the **Orchestration** section:
   - `Start Session`
   - `Status`
   - `Add Lane` / `Add Pane`
   - `Stop Session`

## API Smoke (No Browser)
```bash
curl -sS -X POST http://127.0.0.1:18086/orchestrator/tmux/start \
  -H 'content-type: application/json' \
  -d '{"session":"trace-web-smoke","trace_root":"/tmp/trace-web-smoke","addr":"127.0.0.1:18086"}'

curl -sS -X POST http://127.0.0.1:18086/orchestrator/tmux/status \
  -H 'content-type: application/json' \
  -d '{"session":"trace-web-smoke"}'

curl -sS -X POST http://127.0.0.1:18086/orchestrator/tmux/stop \
  -H 'content-type: application/json' \
  -d '{"session":"trace-web-smoke"}'
```

## Current Status
- Monorepo scaffold is in place (Rust + TypeScript workspace).
- Read-side API projections from canonical event log are implemented.
- tmux orchestration routes are implemented in backend and wired into web UI controls.
- Next milestone is full multi-agent smoke benchmark/eval flow (see active plans).
