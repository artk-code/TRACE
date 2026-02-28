# TRACE Smoke + Eval Plan v2

Date: 2026-02-28  
Depends on: `AGENTS.md`, `BUILD_SEQUENCE_PLAN_v3.md`

## Real-User Smoke Test Target
From a browser UI, run a multi-lane session (Flash/High/Extra), capture trace events on one shared server, and generate a benchmark report that can be reviewed in the same UI.

## Target Flow (Web-Driven)
1. Verify Codex auth status (`available=true`, `logged_in=true`) from web.
2. Start/attach tmux session from web.
3. Spawn configured lanes from web.
4. Execute scripted lane runners (no manual terminal copy/paste).
5. Emit typed write events:
   - claim, run start, output, candidate, release
6. Trigger benchmark evaluation.
7. Retrieve and render report summary in web.

## What Is Already Landed
- CORS contract for browser-origin API access.
- Backend tmux orchestration APIs (`start/status/add-lane/add-pane/stop`).
- Backend Codex auth preflight API (`GET /orchestrator/auth/codex/status`).
- Web tmux orchestration controls wired to those APIs.
- Web blocks lane spawn when Codex auth preflight fails.
- tmux lane launch supports `mode=interactive|runner`.
- Runner mode executes typed writes + `verdict.recorded` automatically.
- Benchmark generation endpoint (`POST /benchmarks/evaluate`) writing JSON/Markdown artifacts.

## Confirmed Gaps
- No coordinated smoke-run job API for multi-lane lifecycle/status.
- tmux add-lane/add-pane routes do not yet hard-require auth server-side (UI currently enforces preflight).
- No smoke-run workflow endpoint coordinating lane lifecycle.
- No report list/get API for UI retrieval.
- No deterministic task pack + expected scoring contract.
- No browser E2E suite verifying end-to-end smoke behavior.

## Milestones
1. M1: Smoke workflow API (trigger + status).
2. M2: Report list/get endpoints.
3. M3: Web report UX (summary + drill-down).
4. M4: Playwright smoke tests + CI gate.

## Acceptance Criteria
- At least 3 lanes can run a web-triggered smoke flow against one server/root.
- No missing or duplicate `global_seq` entries under concurrent lane writes.
- Stale lease writes are rejected/disqualified with explicit reason.
- UI can display benchmark report results without direct filesystem access.
