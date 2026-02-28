# TRACE Smoke + Eval Plan v1

Date: 2026-02-27  
Depends on: `AGENTS.md`, `BUILD_SEQUENCE_PLAN_v2.md`

## Real-User Smoke Test Target
Run multiple Codex terminals (for example Flash, High, Extra profiles) against one TRACE server on one shared workspace.

## Minimal Scenario
1. Start TRACE server with shared `TRACE_ROOT`.
2. Seed a benchmark task set (known expected outputs).
3. Launch N terminals, each tagged with model profile metadata.
4. Each terminal claims a task, executes, emits output and candidate events.
5. TRACE computes compare view and benchmark summary.
6. Report includes:
  - per-model pass/fail and quality scores
  - latency and completion stats
  - stale/disqualified candidate counts

## Required Platform Support
- Write APIs for claims/runs/output/candidates.
- Model/profile metadata in run events.
- Deterministic evaluator harness for known tasks.
- Exportable report artifact (`.trace/reports/<run>.json` + markdown summary).

## Acceptance Criteria
- At least 3 concurrent terminals can complete benchmark run.
- No duplicate/missing `global_seq` in canonical log.
- Lease stale writes are rejected or disqualified with explicit reason.
- Benchmark report is reproducible from logged events.
