# TRACE Smoke + Eval Plan v1

Date: 2026-02-28  
Depends on: `AGENTS.md`, `BUILD_SEQUENCE_PLAN_v2.md`

## Real-User Smoke Test Target
Run multiple Codex lanes (Flash, High, Extra) against one TRACE server and drive the full flow from a web control surface.

## Web-Driven Smoke Scenario (Target)
1. Start TRACE server with shared `TRACE_ROOT`.
2. From web UI, start or attach to a tmux orchestration session.
3. From web UI, spawn lane workers (Flash/High/Extra) with profile metadata.
4. Workers claim tasks, run, emit output and candidate events (no manual paste loop).
5. Web UI triggers benchmark evaluation and fetches resulting report artifacts.
6. Report view includes:
  - per-model pass/fail and quality scores
  - latency and completion stats
  - stale/disqualified candidate counts

## Confirmed Blockers (2026-02-28, Post-Step-2)
- Resolved: local web-to-API connectivity now has backend CORS contract and regression coverage.
- Resolved: backend orchestration API route surface now exists for tmux control endpoints.
- Web app currently has no orchestration or write actions.
- Lane execution remains manual in pane shells.
- Report retrieval/list endpoints for UI are missing.
- Deterministic evaluator task pack/scoring is not yet implemented.
- CI lacks web-driven E2E smoke coverage.

## Required Platform Support (v1)
- Web/API connectivity contract (Vite proxy or CORS policy).
- Orchestration control plane endpoints:
  - start/status/add-lane/add-pane/stop.
- Non-interactive lane runner mode for scripted smoke execution.
- Typed writer APIs for claim/run/output/candidate/release with lease fencing.
- Report generation + retrieval:
  - `POST /benchmarks/evaluate`
  - report list/get routes for UI consumption.
- Deterministic evaluator harness for seeded tasks.

## Milestones
1. M0: Connectivity and backend control plane.
  - Backend CORS + tmux control routes are implemented.
  - Remaining gap: web UI/client is not wired to those endpoints yet.
2. M1: Scripted lanes.
  - One-click smoke run spawns Flash/High/Extra lanes and writes events.
3. M2: Report UX.
  - UI can trigger benchmark generation and render per-model summary.
4. M3: Deterministic scoring.
  - Pass/fail and quality derived from seeded expected outputs.
5. M4: CI gate.
  - Automated web-smoke run enforced in CI.

## Acceptance Criteria
- At least 3 concurrent lanes can complete a web-triggered benchmark run.
- No duplicate/missing `global_seq` in canonical log.
- Lease stale writes are rejected or disqualified with explicit reason.
- Benchmark report is reproducible from logged events and retrievable by the UI.
