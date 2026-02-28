# TRACE Build Sequence Plan v2

Date: 2026-02-28  
Depends on: `AGENTS.md`

## Goal
Move from read-focused scaffold + manual tmux orchestration to a web-driven smoke benchmark system.

## Sequence
1. Web/API transport contract.
  - Lock local-dev connectivity strategy (Vite proxy or CORS). ✅ (CORS landed)
  - Add a regression check for browser-origin calls. ✅
2. Orchestration control-plane routes.
  - Add backend endpoints for tmux session lifecycle:
    - start/status/add-lane/add-pane/stop. ✅
  - Enforce input validation for session/lane/profile/target. ✅
3. Scripted lane runner mode.
  - Keep interactive mode for humans.
  - Add non-interactive lane mode for smoke automation.
4. Web smoke actions.
  - Add UI controls to trigger orchestration actions and benchmark evaluation.
  - Add state display for active sessions/lanes.
5. Benchmark artifact retrieval.
  - Keep current report generation.
  - Add report list/get endpoints for UI rendering.
6. Deterministic evaluator.
  - Seed known task set with expected outputs.
  - Score pass/fail/quality deterministically.
7. E2E + CI expansion.
  - Add web-driven smoke test that validates event ingest + report output.
  - Gate CI on that smoke flow.

## Blocking Risks
- Web UI/client may drift from backend orchestration contracts if not wired with typed requests.
- Scripted lane mode can race or mis-sequence writes if not fenced by lease checks.
- Benchmark report remains non-authoritative until deterministic evaluator ships.
- Web smoke can be flaky in CI without stable process lifecycle + cleanup.

## Required Regression Gates
- `rustup run stable cargo fmt --all --check`
- `rustup run stable cargo clippy --workspace --all-targets -- -D warnings`
- `rustup run stable cargo test --workspace`
- `pnpm --dir web test`
- `pnpm --dir web build`
- web-driven smoke E2E (to be added as CI gate)
