# TRACE Linux Build and Test Guide (Human Operators)

Date: 2026-03-01  
Target OS: Ubuntu 22.04 / 24.04 LTS

## 1. System Dependencies
```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  curl git tmux jq ca-certificates
```

## 2. Rust Toolchain
```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
```

## 3. Node.js + pnpm
```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
corepack enable
corepack prepare pnpm@9 --activate
```

## 4. Repository Setup
```bash
cd /path/to/TRACE
pnpm install
```

## 5. Playwright Browser Runtime
Required before running browser E2E locally. `--with-deps` installs browser system dependencies on Ubuntu.
```bash
pnpm --dir web exec playwright install --with-deps chromium
```

## 6. Full Regression Matrix
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir web test
pnpm --dir web build
pnpm --dir web test:e2e
```

## 7. Common Issues
- `pnpm: command not found`:
  - run `corepack enable` and re-open shell.
- Playwright browser missing:
  - run `pnpm --dir web exec playwright install --with-deps chromium`.
- `tmux` preflight failures during smoke:
  - verify session/target with:
  - `scripts/trace-smoke-tmux.sh --session trace-smoke status`
  - `scripts/trace-smoke-tmux.sh --session trace-smoke validate-target trace-smoke:lanes`
- Codex auth failures:
  - `codex login`
  - `curl -sS http://127.0.0.1:18086/orchestrator/auth/codex/status | jq .`
