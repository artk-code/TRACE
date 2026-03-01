# TRACE Build Sequence Plan v3

Date: 2026-03-01  
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
9. Smoke workflow API is implemented:
   - `POST /smoke/runs`
   - `GET /smoke/runs/{run_id}`
   - preflights tmux session/target, scopes events by smoke lanes, writes benchmark report at completion
10. Report retrieval APIs are implemented:
   - `GET /reports`
   - `GET /reports/{report_id}`
11. Web UI smoke/report flow is implemented:
   - `Run Smoke`
   - `Refresh Status`
   - `View Latest Report`
12. Browser E2E smoke is implemented and CI-gated:
   - `web/tests/phase0-smoke.spec.ts`
   - `.github/workflows/ci.yml` runs `pnpm test:e2e`

## Execution Sequence
1. Phase 0 human QA sign-off.
   - Execute the manual browser/tmux validation flow in `docs/PHASE0_HUMAN_QA.md`.
   - Record evidence in `docs/PHASE0_SIGNOFF.md`.
2. Deterministic eval contract.
   - Add seeded task pack and expected-output checks.
   - Make benchmark quality outcome reproducible run-to-run.
3. Merge/PR pipeline.
   - Add winner/stacked-candidate export and Git-compatible PR workflow after smoke path stabilizes.

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
- `pnpm --dir web test:e2e`

## Human QA Gate
- Run manual Phase 0 smoke verification with a real TRACE server + tmux target.
- Capture run artifacts and verdict in `docs/PHASE0_SIGNOFF.md`.

## Exit Criteria
- One web-driven action can run a 3-lane smoke scenario end-to-end.
- Benchmark report is generated and retrievable/renderable in UI.
- Browser E2E smoke passes locally and in CI.
