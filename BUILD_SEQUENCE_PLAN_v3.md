# TRACE Build Sequence Plan v3

Date: 2026-02-28  
Depends on: `AGENTS.md`

## Goal
Ship a browser-driven, repeatable smoketest where multiple lanes (Flash/High/Extra) are orchestrated from web controls, emit trace events, and produce benchmark reports visible in the UI.

## Current State (Completed)
1. Browser/API transport contract is in place (CORS + tests).
2. Backend tmux orchestration control plane is implemented:
   - `POST /orchestrator/tmux/start|status|add-lane|add-pane|stop`
3. Web UI has tmux orchestration controls wired to backend APIs.
4. Codex auth preflight endpoint is implemented:
   - `GET /orchestrator/auth/codex/status`
5. Web UI can run auth preflight and blocks lane spawn if Codex auth is missing.
6. Backend enforces Codex auth policy on lane spawn:
   - `TRACE_CODEX_AUTH_POLICY=required|optional` (default `required`)
   - `add-lane`/`add-pane` return precondition failure when auth is required and not logged in
7. Core write-path fencing is in place:
   - lock-safe event append
   - lease epoch/holder validation for typed write routes
8. Scripted lane runner mode is available:
   - tmux add-lane/add-pane accept `mode=interactive|runner`
   - runner mode emits claim/run/output/candidate/verdict/release automatically

## Execution Sequence
1. Smoke workflow endpoint.
   - Add API workflow to launch/coordinate Flash/High/Extra runner lanes for a predefined task pack.
   - Return workflow/job state for UI polling.
2. Benchmark report retrieval APIs.
   - Add list/get endpoints for report artifacts under `.trace/reports`.
   - Keep report ID sanitization and root scoping safeguards.
3. Web smoke dashboard.
   - Add "Run Smoke" + "Evaluate" controls.
   - Render report summary table (per model pass/fail, durations, stale/disqualified counts).
4. Browser E2E harness.
   - Add Playwright smoke that verifies:
     - Codex auth preflight visible and required for lane spawn
     - backend policy enforcement (`required` blocks unauthenticated lane spawn)
     - tmux start/status from UI
     - smoke run triggers event writes
     - benchmark report appears in UI
5. CI gate.
   - Run Rust + web regression + Playwright smoke.
   - Fail build on smoke regression.

## Risks
- Runner concurrency can race lease transitions if workers are not sequenced per task/epoch.
- tmux orchestration can fail on host capability differences (session naming, pane targets).
- Report retrieval can expose filesystem paths if routes are not scoped/sanitized.
- Browser smoke can be flaky without deterministic task seeds and controlled timing.

## Required Regression Gates
- `rustup run stable cargo fmt --all --check`
- `rustup run stable cargo clippy --workspace --all-targets -- -D warnings`
- `rustup run stable cargo test --workspace`
- `pnpm --dir web test`
- `pnpm --dir web build`
- `pnpm --dir web test:e2e` (to be added with Playwright)

## Exit Criteria
- One web-driven action can run a 3-lane smoke scenario end-to-end.
- Benchmark report is generated and retrievable/renderable in UI.
- Browser E2E smoke passes locally and in CI.
