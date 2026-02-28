# TRACE Frontend Plan v3

Date: 2026-02-28  
Depends on: `AGENTS_v5.md`

## Goal
Deliver UI increments that align with finalized backend contracts and avoid contract churn.

---

## Contract assumptions (must match backend)

1. Task API shape is nested:
   - `TaskResponse = { task: Task, status: TaskStatus, status_detail?: object }`
2. Task status is server-computed and authoritative.
3. Candidate/evaluation lists exclude stale/disqualified epochs by default.
4. Run/task timelines are read from backend projections derived from global event log.

If any assumption changes, frontend implementation pauses until specs are updated.

---

## Data model in UI

```ts
type TaskResponse = {
  task: {
    task_id: string;
    title: string;
    owner?: string;
  };
  status: "Unclaimed" | "Claimed" | "Running" | "Evaluating" | "Reviewed" | "Done";
  status_detail?: {
    lease_epoch?: number;
    holder?: string;
    reason?: string;
  };
};
```

Additional view models:

- `TimelineEvent`: includes `kind`, `ts`, `task_id`, optional `run_id`
- `CandidateSummary`: includes `candidate_id`, `run_id`, `lease_epoch`, `eligible: boolean`

UI defaults to showing `eligible=true` unless user toggles “Show stale/disqualified”.

---

## Phased rollout

### Phase 0 — Contract lock + scaffolding
- Generate/handwrite API client types from canonical schema.
- Remove any flat task-response assumptions.
- Add runtime guards (zod/io-ts/etc.) to fail fast on contract drift.

### Phase 1 — Task board and status surfaces
- Task list and task detail consume only `status` + `status_detail` from API.
- No client-side recomputation of lifecycle state.
- Display lease metadata when `Claimed/Running`.

### Phase 2 — Timeline views
- Task timeline supports task-level events before first run (`task.claimed`, `verdict.recorded`).
- Run timeline remains available as projection filtered by `run_id`.

### Phase 3 — Candidate/evaluation UX
- Candidate lists and compare views default to eligible candidates only.
- Add badge + toggle for stale/disqualified candidates.

### Phase 4 — Output rendering
- Render `runner.output` chunks with `encoding` handling:
  - `utf8`: direct text append
  - `base64`: decode safely with size guards
- Use incremental buffering in UI to avoid large reflow on high-output runs.

---

## API endpoints expected by frontend

- `GET /tasks` → `TaskResponse[]`
- `GET /tasks/:task_id` → `TaskResponse`
- `GET /tasks/:task_id/timeline` → task-scoped events (task + run events)
- `GET /runs/:run_id/timeline` → run-projection events
- `GET /tasks/:task_id/candidates?include_disqualified=false`
- `GET /runs/:run_id/output` (chunked/event-stream or paged API)

---

## Integration risks to watch

1. **Contract drift risk**: flat vs nested response reintroduced in sample payloads.
2. **Status drift risk**: accidental client-side inference logic.
3. **Stale candidate leakage**: disqualified candidates shown as normal by default.
4. **Output perf risk**: unbounded DOM growth from large output streams.

Mitigations: schema tests, component tests for stale filtering, and output virtualization.

---

## Definition of done

- All frontend types compile against nested `TaskResponse` only.
- No component computes task lifecycle from raw events.
- Timeline supports pre-run task events.
- Candidate UI clearly distinguishes eligible vs stale/disqualified.
- Output panel passes large-stream performance sanity checks.
