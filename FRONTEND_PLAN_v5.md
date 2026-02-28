# TRACE Frontend Plan v5

Date: 2026-02-28  
Depends on: `AGENTS.md`, `SMOKETEST_EVAL_PLAN_v2.md`

## Goal
Move from read-focused UI to browser-driven smoke orchestration and benchmark reporting.

## Current Frontend Baseline
- Runtime guards exist for task/candidate/output contracts.
- UI reads tasks/candidates/run output.
- UI can invoke tmux orchestration actions:
  - start/status/add-lane/add-pane/stop
- UI has Codex auth preflight check (`GET /orchestrator/auth/codex/status`).
- UI gating follows backend policy:
  - `policy=required` blocks lane spawn when unauthenticated
  - `policy=optional` allows lane spawn
- Backend smoke workflow API is available for UI integration:
  - `POST /smoke/runs`
  - `GET /smoke/runs/{run_id}`

## Phase Sequence
1. Phase A: Workflow trigger + status polling.
   - Add `Run Smoke` action backed by `POST /smoke/runs`.
   - Poll `GET /smoke/runs/{run_id}` and render state: idle/running/succeeded/failed.
2. Phase B: Report fetch + summary.
   - Add `View Latest Report` backed by `GET /reports` + `GET /reports/{report_id}`.
   - Render per-model summary table (pass/fail/duration/disqualified).
3. Phase C: Report drill-down.
   - Show per-run rows and filtering by model/profile.
4. Phase D: Browser E2E.
   - Add Playwright for auth check -> run smoke -> report visible path.

## Frontend Test Tracks
- Unit (`vitest`): schema/guard and helper logic.
- Component (`testing-library`, to add):
  - auth preflight status and error/remediation rendering
  - smoke run trigger + polling state transitions
  - report table rendering with fixture payloads
- E2E (`playwright`, to add):
  - open app, verify auth gate behavior, run smoke workflow, assert report UI

## Definition Of Done
- Browser can drive smoke run and view benchmark summary end-to-end.
- UI shows actionable errors for orchestration/report failures.
- E2E smoke is stable enough for CI gating.
