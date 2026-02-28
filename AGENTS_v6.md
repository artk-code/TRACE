# TRACE Agent Specification v6

Date: 2026-02-28  
Status: **Locked for Phase 0 scaffold**

## Purpose
This document defines the backend/source-of-truth contracts for event logging, leases, task status, and API envelopes so implementation can proceed without foundational rewrites.

---

## 0) Phase 0 Backbone Decisions

### Decision
Phase 0 uses a Rust + TypeScript monorepo with a local-first CLI + local API publish target for v0.1.

### Locked implementation defaults
- Rust workspace crates:
  - `trace-events`
  - `trace-store`
  - `trace-lease`
  - `trace-normalizer`
  - `trace-api-types`
  - `trace-server`
  - `trace-cli`
- Frontend package:
  - `web` (TypeScript + React + Vite; read-heavy consumer of API contracts)
- Canonical event log path:
  - `.trace/events/events.jsonl`
- Derived run projection:
  - `.trace/traces/<run_id>.jsonl`
- Replay-before-guard startup rule:
  - replay unapplied events to checkpoint tip before enabling lease-sensitive operations.
- Candidate eligibility/disqualification:
  - stale epochs are disqualified with `disqualified_reason=stale_epoch`.
- `runner.output` encoding semantics:
  - payload must include `encoding: utf8 | base64`.

### Phase 0 REST endpoint set
- `GET /tasks`
- `GET /tasks/:task_id`
- `GET /tasks/:task_id/timeline`
- `GET /runs/:run_id/timeline`
- `GET /tasks/:task_id/candidates?include_disqualified=false`
- `GET /runs/:run_id/output`

---

## 1) Canonical Event Model

### Decision
TRACE uses a **global append-only event log** as canonical durability surface:

- Canonical log file: `.trace/events/events.jsonl`
- Canonical ordering: `global_seq` (assigned on append, strictly increasing)
- Producers submit events without sequence; storage assigns `global_seq`

Per-run files are retained as **derived projections** for developer ergonomics and debugging:

- Derived run projection: `.trace/traces/<run_id>.jsonl`
- Rebuilt by normalization/reprojection from global log
- `RunStarted`-first ordering is a run-projection invariant, not a global-log invariant

### Rationale
This resolves the inability to represent task-level events (`task.claimed`, `task.renewed`, `task.released`, `verdict.recorded`) when no run exists.

### Required event envelope
```json
{
  "global_seq": 1842,
  "ts": "2026-02-28T05:20:18.123Z",
  "task_id": "TASK-42",
  "run_id": "RUN-13",      // nullable for task-only events
  "kind": "task.claimed",
  "payload": {"epoch": 7, "worker_id": "agent-3"}
}
```

- `run_id` is nullable in canonical log.
- Event constructors must use a pre-write type with `global_seq: None`; persisted type always has assigned `global_seq`.

---

## 2) Lease Authority and Crash Consistency

### Decision
Leases are **derived operational state** from canonical events, with mandatory replay before guard/claim decisions.

- Source of durability truth: global JSONL event log
- Operational index/cache: SQLite `leases` table
- On startup, before serving lease-sensitive operations:
  1. replay unapplied global events into SQLite,
  2. verify replay checkpoint reaches tip,
  3. then enable WorkspaceGuard and claim APIs.

### Durability policy
- Event append durability remains primary durability mechanism.
- SQLite may run with performance-oriented pragmas because it is replayable.
- WorkspaceGuard must reject lease-sensitive operations if replay is behind.

This preserves no-stale-write fencing guarantees without requiring SQLite to be independently authoritative.

---

## 3) Fencing Rules for ChangeSets / Evaluation

### Decision
Task status transitions that depend on candidate outputs must be filtered by **eligible candidate** semantics.

### Eligible candidate definition
A candidate (`changeset.created`, evaluation events, verdict proposal) is eligible iff:

1. event carries `(task_id, run_id, lease_epoch)`,
2. there exists lease history proving `run_id` held `task_id` at `lease_epoch`,
3. the epoch is current at the candidate creation point (not superseded), or candidate is explicitly marked `stale`.

### Enforcement
- Writer path SHOULD reject stale `changeset.created` at ingestion where possible.
- If stale events are ingested (race/crash/replay), normalizer must classify them `disqualified_reason=stale_epoch`.
- `TaskStatus` (`Evaluating`, `Reviewed`) and compare views must ignore disqualified candidates by default.

---

## 4) API Contract Shape (Backend ↔ Frontend)

### Decision
Use the **nested** canonical task response shape everywhere.

```json
{
  "task": {
    "task_id": "TASK-42",
    "title": "Improve lease replay",
    "owner": "platform"
  },
  "status": "Claimed",
  "status_detail": {
    "lease_epoch": 7,
    "holder": "agent-3"
  }
}
```

- Flat examples are deprecated.
- API docs, mocks, tests, and frontend types must all reference this shape.

---

## 5) `runner.output` Event Schema + Throughput Policy

### Decision
`runner.output` schema includes explicit encoding discriminator.

Required payload fields:

- `stream`: `stdout | stderr`
- `encoding`: `utf8 | base64`
- `chunk`: string
- `chunk_index`: monotonically increasing per stream per run
- `final`: boolean (optional, true on terminal chunk)

### Durability/performance policy
- Output is buffered and emitted in bounded chunks (target ~16–64 KiB).
- Storage performs durability sync per append batch/window, **not per tiny chunk**.
- Extremely high-volume output may be redirected to an append-only artifact file with hash-linked manifest events.

---

## 6) Type Consistency Fix

Clarify model split:

- `NewTraceEvent` (producer-side): `global_seq: Option<u64>` (must be `None` before persist)
- `TraceEvent` (persisted/read-side): `global_seq: u64` (always set)

This removes the prior contradiction.

---

## 7) Non-Goals (unchanged)

The following prior design points remain valid:

- workspace/crate decomposition and dependency boundaries,
- workspace lifecycle separated from VCS operations,
- server-computed task status as UI contract,
- phased endpoint rollout,
- frontend as read-heavy consumer of store APIs.

---

## 8) Implementation Gate v6

Implementation may proceed when all checklist items are true and their corresponding tests pass.

- [ ] Global event log path + schema merged and enforced.
  - Pass checks:
    - `test_event_log_path_is_canonical`
    - `test_trace_event_requires_global_seq_on_persisted_reads`
- [ ] Replay-before-guard startup contract implemented.
  - Pass checks:
    - `test_replay_gate_blocks_claim_when_checkpoint_behind`
    - `test_replay_gate_enables_claim_after_tip_reached`
- [ ] Eligible-candidate filtering implemented in normalizer/status views.
  - Pass checks:
    - `test_stale_epoch_candidate_marked_disqualified`
    - `test_candidate_views_exclude_disqualified_by_default`
- [ ] Nested `TaskResponse` reflected in all docs/examples and API responses.
  - Pass checks:
    - `test_task_response_accepts_nested_shape`
    - `test_task_response_rejects_flat_shape`
- [ ] `runner.output.encoding` added and chunking policy documented.
  - Pass checks:
    - `test_runner_output_requires_encoding_field`
    - `test_runner_output_base64_chunk_decode_with_size_guard`
- [ ] `NewTraceEvent` vs `TraceEvent` type split reflected in code/docs.
  - Pass checks:
    - `test_new_trace_event_global_seq_is_none_before_persist`
    - `test_trace_event_global_seq_is_set_after_persist`
