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
- UI has Codex auth preflight check (`GET /orchestrator/auth/codex/status`) and blocks lane spawn if unauthenticated.

## Phase Sequence
1. Phase A: Auth gate hardening.
   - Keep explicit "Check Codex Auth" status panel.
   - Add clear remediation commands when unauthenticated (`codex login`, `--device-auth`, API key login).
2. Phase B: Smoke actions.
   - Add web action to trigger smoke workflow API (once backend exists).
   - Add explicit UI state machine: idle/running/succeeded/failed.
3. Phase C: Benchmark controls.
   - Add evaluate trigger and report selector.
   - Add per-model summary table (pass/fail/duration/disqualified).
4. Phase D: Report drill-down.
   - Show per-run rows and basic filtering by model/profile.
5. Phase E: Browser E2E.
   - Add Playwright for UI smoke flow.
   - Validate auth preflight + orchestration action + report render path.

## Frontend Test Tracks
- Unit (`vitest`): schema/guard and helper logic.
- Component (`testing-library`, to add):
  - auth preflight status and error/remediation rendering
  - add-lane/add-pane preflight blocking
  - orchestration action state transitions
  - report table rendering with fixture payloads
- E2E (`playwright`, to add):
  - open app, verify auth gate behavior, trigger orchestration status, trigger evaluate, assert report UI

## Definition Of Done
- Browser can drive smoke run and view benchmark summary end-to-end.
- UI shows actionable errors for orchestration/report failures.
- E2E smoke is stable enough for CI gating.
