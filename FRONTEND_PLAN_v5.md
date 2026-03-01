# TRACE Frontend Plan v5

Date: 2026-03-01  
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
- UI smoke workflow controls are implemented:
  - `Run Smoke`
  - `Refresh Status`
  - active-run status polling
- UI report flow is implemented:
  - `View Latest Report` via `GET /reports` + `GET /reports/{report_id}`
  - model summary table rendering from benchmark report payload

## Phase Sequence
1. Phase A: Workflow trigger + status polling. (Completed)
2. Phase B: Report fetch + summary. (Completed)
3. Phase C: Report drill-down. (Pending)
   - Show per-run rows and filtering by model/profile.
4. Phase D: Browser E2E. (Pending)
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
