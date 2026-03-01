# TRACE Smoke + Eval Plan v2

Date: 2026-03-01  
Depends on: `AGENTS.md`, `BUILD_SEQUENCE_PLAN_v3.md`

## Real-User Smoke Test Target
From a browser UI, run a multi-lane session (Flash/High/Extra), capture trace events on one shared server, and generate a benchmark report that can be reviewed in the same UI.

## Target Flow (Web-Driven)
1. Verify Codex auth status from web.
   - When `policy=required`, require `available=true` and `logged_in=true`.
2. Trigger smoke workflow via `POST /agent/runs` (legacy alias: `/smoke/runs`).
3. Poll `GET /agent/runs/{run_id}` until terminal state.
4. Execute scripted lane runners (no manual terminal copy/paste).
5. Emit typed write events:
   - claim, run start, output, candidate, release
6. Trigger benchmark evaluation inside workflow completion path.
7. Retrieve report via report APIs and render summary in web.

## What Is Already Landed
- CORS contract for browser-origin API access.
- Backend tmux orchestration APIs (`start/status/add-lane/add-pane/stop`).
- Backend Codex auth preflight API (`GET /orchestrator/auth/codex/status`).
- Backend auth policy enforcement on lane spawn (`TRACE_CODEX_AUTH_POLICY=required|optional`, default `required`).
- Web tmux orchestration controls wired to those APIs.
- Web auth preflight mirrors backend policy behavior for operator feedback.
- tmux lane launch supports `mode=interactive|runner`.
- Runner mode executes typed writes + `verdict.recorded` automatically.
- Benchmark generation endpoint (`POST /benchmarks/evaluate`) writing JSON/Markdown artifacts.
- Smoke workflow endpoints are active:
  - `POST /agent/runs` (legacy alias: `/smoke/runs`)
  - `GET /agent/runs/{run_id}` (legacy alias: `/smoke/runs/{run_id}`)
  - preflights session/target and writes benchmark summary on completion
- Report retrieval endpoints are active:
  - `GET /reports`
  - `GET /reports/{report_id}`
- Web UI smoke/report flow is active:
  - run agents
  - poll status
  - view latest report summary
- Browser E2E smoke is active:
  - Playwright baseline for auth check -> smoke run -> report visible
  - CI runs `pnpm --dir web test:e2e`
- Browser JJ workflow panel is active:
  - bootstrap/status/lane management/patch/publish/integrate actions via `/orchestrator/jj/*`

## Confirmed Gaps
- Human QA sign-off artifacts for a real shared-server run are not yet recorded.
- No deterministic task pack + expected scoring contract.

## Milestones
1. M2: Report retrieval APIs. (Completed)
2. M3: Minimal web flow. (Completed)
3. M4: Deterministic evaluator seed pack. (Pending)
4. M5: Playwright smoke tests + CI gate. (Completed)
5. M6: Human QA sign-off record with real tmux/session run evidence. (Pending)

## Acceptance Criteria
- At least 3 lanes can run a web-triggered smoke flow against one server/root.
- No missing or duplicate `global_seq` entries under concurrent lane writes.
- Stale lease writes are rejected/disqualified with explicit reason.
- UI can display benchmark report results without direct filesystem access.

## Human QA Evidence
- Manual execution checklist lives in `docs/PHASE0_HUMAN_QA.md`.
- Final sign-off record lives in `docs/PHASE0_SIGNOFF.md`.
