# TRACE Setup Guide (Ubuntu Linux)

Date: 2026-03-01  
Target OS: Ubuntu 22.04 / 24.04 LTS

## 1. System Dependencies
```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  curl git tmux jq ca-certificates xvfb
```

## 2. Codex CLI Prerequisite
TRACE lane spawn and agent runs are blocked when `TRACE_CODEX_AUTH_POLICY=required` and Codex is not logged in.

Verify Codex CLI is available:
```bash
codex --version
codex login status
```

If Codex is installed outside `PATH`, set:
```bash
export TRACE_CODEX_BIN=/absolute/path/to/codex
```

## 3. Rust Toolchain
```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
```

## 4. Node.js + pnpm
```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
corepack enable
corepack prepare pnpm@9 --activate
```

## 5. Repository Setup
```bash
cd /path/to/TRACE
pnpm install
```

## 6. Playwright Runtime (Linux)
Required before running browser E2E locally. `--with-deps` installs browser runtime dependencies on Ubuntu.
```bash
pnpm --dir web exec playwright install --with-deps chromium
```

Headless-only hosts can run:
```bash
xvfb-run -a pnpm --dir web test:e2e
```

## 7. Full Regression Matrix
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir web test
pnpm --dir web build
pnpm --dir web test:e2e
```

## 8. Local Operator Flow (Ubuntu)

1. Start TRACE server:
```bash
TRACE_SERVER_ADDR=127.0.0.1:18086 \
TRACE_ROOT=/tmp/trace-web-smoke \
TRACE_CODEX_AUTH_POLICY=required \
cargo run -p trace-server
```

2. Start web UI in another terminal:
```bash
VITE_TRACE_API_BASE_URL=http://127.0.0.1:18086 \
pnpm --dir web dev --host 127.0.0.1 --port 4173
```

3. Start tmux session:
```bash
scripts/trace-smoke-tmux.sh --session trace-smoke start --no-attach
```

4. Verify terminal workspace endpoints:
```bash
curl -sS -X POST http://127.0.0.1:18086/orchestrator/tmux/snapshot \
  -H 'content-type: application/json' \
  -d '{"session":"trace-smoke"}' | jq .

curl -sS -X POST http://127.0.0.1:18086/orchestrator/tmux/capture \
  -H 'content-type: application/json' \
  -d '{"session":"trace-smoke","target":"trace-smoke:lanes.0","lines":120}' | jq .

curl -sS -X POST http://127.0.0.1:18086/orchestrator/tmux/send-keys \
  -H 'content-type: application/json' \
  -d '{"session":"trace-smoke","target":"trace-smoke:lanes.0","text":"echo linux-proof","press_enter":true}' | jq .
```

5. Trigger and poll agent run:
```bash
RUN_ID="$(curl -sS -X POST http://127.0.0.1:18086/agent/runs \
  -H 'content-type: application/json' \
  -d '{"session":"trace-smoke","target":"trace-smoke:lanes"}' | jq -r '.run_id')"

while true; do
  STATUS="$(curl -sS "http://127.0.0.1:18086/agent/runs/$RUN_ID" | jq -r '.status')"
  if [[ "$STATUS" == "succeeded" || "$STATUS" == "failed" ]]; then
    break
  fi
  sleep 1
done
```

6. Stop tmux session when done:
```bash
scripts/trace-smoke-tmux.sh --session trace-smoke stop
```

## 9. Common Issues
- `pnpm: command not found`:
  - run `corepack enable`, then restart shell.
- Playwright browser/runtime missing:
  - run `pnpm --dir web exec playwright install --with-deps chromium`.
- `tmux` preflight failures:
  - `scripts/trace-smoke-tmux.sh --session trace-smoke status`
  - `scripts/trace-smoke-tmux.sh --session trace-smoke validate-target trace-smoke:lanes`
- `send-keys` rejected with `400`:
  - provide at least one of `text`, `key`, or `press_enter=true`.
  - allowed keys: `Enter`, `Tab`, `BSpace`, `Escape`, `Up`, `Down`, `Left`, `Right`, `C-c`, `C-z`, `C-l`, `C-u`.
- Codex auth failures:
  - `codex login`
  - `curl -sS http://127.0.0.1:18086/orchestrator/auth/codex/status | jq .`
