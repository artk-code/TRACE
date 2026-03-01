# TRACE Frontend Plan v5

Date: 2026-03-01  
Depends on: `AGENTS.md`, `SMOKETEST_EVAL_PLAN_v2.md`

## Goal
Move from read-focused UI to browser-driven agent orchestration and benchmark reporting.

## Current Frontend Baseline
- Runtime guards exist for task/candidate/output contracts.
- UI reads tasks/candidates/run output.
- UI can invoke tmux orchestration actions:
  - start/status/add-lane/add-pane/stop
- UI has Codex auth preflight check (`GET /orchestrator/auth/codex/status`).
- UI gating follows backend policy:
  - `policy=required` blocks lane spawn when unauthenticated
  - `policy=optional` allows lane spawn
- Backend agent workflow API is available for UI integration:
  - `POST /agent/runs` (legacy alias: `/smoke/runs`)
  - `GET /agent/runs/{run_id}` (legacy alias: `/smoke/runs/{run_id}`)
- UI agent workflow controls are implemented:
  - `Run Agents`
  - `Refresh Status`
  - active-run status polling
- UI report flow is implemented:
  - `View Latest Report` via `GET /reports` + `GET /reports/{report_id}`
  - model summary table rendering from benchmark report payload
- UI JJ workflow controls are implemented:
  - `JJ Bootstrap`, `JJ Status`
  - `Lane Add`, `Lane List`, `Lane Root`, `Lane Forget`
  - `Export Patch`, `Publish`, `Integrate`

## Phase Sequence
1. Phase A: Agent workflow trigger + status polling. (Completed)
2. Phase B: Report fetch + summary. (Completed)
3. Phase C: Report drill-down. (Pending)
   - Show per-run rows and filtering by model/profile.
4. Phase D: Browser E2E. (Completed)
   - Add Playwright for auth check -> run smoke -> report visible path.
   - Wire CI to run `pnpm --dir web test:e2e`.
5. Phase E: JJ control surface + validation. (Completed)
   - Add backend/API/UI integration for `/orchestrator/jj/*`.
   - Add Playwright coverage for JJ action payload wiring.

## Frontend Test Tracks
- Unit (`vitest`): schema/guard and helper logic.
- Component (`testing-library`, to add):
  - auth preflight status and error/remediation rendering
  - agent run trigger + polling state transitions
  - report table rendering with fixture payloads
- E2E (`playwright`):
  - open app, verify auth gate behavior, run agent workflow, assert report UI
  - status: baseline landed in `web/tests/phase0-smoke.spec.ts`
  - jj control surface payload verification in `web/tests/jj-workflow.spec.ts`
  - note: CI baseline is API-stubbed; real server/tmux verification is tracked in `docs/PHASE0_HUMAN_QA.md`

## Definition Of Done
- Browser can drive agent run and view benchmark summary end-to-end.
- UI shows actionable errors for orchestration/report failures.
- E2E smoke is stable enough for CI gating. (Completed on 2026-03-01)
