# TRACE Prebug Checklist v1

Date: 2026-02-28  
Depends on: `AGENTS_v6.md`, `FRONTEND_PLAN_v4.md`, `PHASE0_FINALE_PLAN_v1.md`  
Status: **Structured review checklist for pre-implementation bug discovery**

## Severity tags
- `P0 must-fix`: blocks implementation start.
- `P1 should-fix`: high-value correction before broad coding.
- `P2 note`: non-blocking but should be tracked.

---

## How to use this checklist

1. Review each subsystem section and mark every item `PASS`, `FAIL`, or `N/A`.
2. Create one finding per failed item using the review output template.
3. Implementation begins only when all `P0` items are `PASS` or have approved mitigation.

---

## Backend invariants checks

### Event log and sequencing
- `[P0 must-fix]` Canonical log path is documented consistently as `.trace/events/events.jsonl`.
- `[P0 must-fix]` `TraceEvent.global_seq` is required on persisted/read-side contracts.
- `[P0 must-fix]` Sequence assignment is strictly monotonic and append-time assigned.
- `[P1 should-fix]` Derived run projections are explicitly marked non-canonical.

### Replay checkpoint and guard behavior
- `[P0 must-fix]` Startup contract requires replay-to-tip before lease-sensitive operations.
- `[P0 must-fix]` Guard behavior is explicit when replay is behind (reject/closed-by-default).
- `[P1 should-fix]` Checkpoint/tip terminology is unambiguous across all docs.
- `[P2 note]` Performance pragmas for replayable SQLite cache are documented without changing durability authority.

### Stale epoch disqualification
- `[P0 must-fix]` Eligible candidate definition includes `(task_id, run_id, lease_epoch)` linkage.
- `[P0 must-fix]` Stale candidate classification uses `disqualified_reason=stale_epoch`.
- `[P0 must-fix]` Status/compare semantics exclude disqualified candidates by default.
- `[P1 should-fix]` Writer-path stale rejection guidance is documented as SHOULD with fallback normalizer behavior.

---

## API contract checks

### TaskResponse and shape stability
- `[P0 must-fix]` Canonical `TaskResponse` shape is nested (`task`, `status`, `status_detail`).
- `[P0 must-fix]` Flat response examples are absent from current canonical docs.
- `[P1 should-fix]` Field naming conventions are consistent (`task_id`, `status_detail`, etc.).

### Nullability and payload clarity
- `[P0 must-fix]` `run_id` nullability is explicitly documented for task-only events.
- `[P1 should-fix]` Optional fields are clearly marked optional vs nullable where relevant.
- `[P1 should-fix]` `runner.output` required payload fields are enumerated exactly.

### Endpoint set consistency
- `[P0 must-fix]` Phase 0 endpoint list is identical across AGENTS, frontend plan, and finale plan.
- `[P1 should-fix]` Query parameter defaults (like `include_disqualified=false`) are not contradictory.

---

## Frontend checks

### Lifecycle/status authority
- `[P0 must-fix]` Docs forbid client-side lifecycle recomputation from raw events.
- `[P1 should-fix]` Status rendering behavior references only server `status` and `status_detail`.

### Candidate visibility behavior
- `[P0 must-fix]` Disqualified candidates are hidden by default.
- `[P1 should-fix]` Explicit toggle behavior for showing stale/disqualified candidates is documented.

### Output handling safeguards
- `[P0 must-fix]` Frontend decoding path distinguishes `utf8` and `base64`.
- `[P0 must-fix]` Base64 decoding includes safety guardrails for malformed or oversized chunks.
- `[P1 should-fix]` Output rendering strategy addresses DOM growth/reflow risk.

---

## CI and release checks

### Required jobs and merge gates
- `[P0 must-fix]` Required CI jobs include backend tests, frontend tests, API E2E, and UI smoke.
- `[P1 should-fix]` Job naming and scope are consistent between docs and pipeline plan.

### Artifact and reproducibility expectations
- `[P0 must-fix]` v0.1 artifact target is clear: local-first CLI + local API.
- `[P1 should-fix]` Release process includes checksums/signatures expectation.
- `[P1 should-fix]` Toolchain/lockfile determinism expectations are documented.

---

## Required prebug scenario coverage

1. Replay gate blocks lease-sensitive operations when behind tip.
2. Replay completion enables claim APIs and guard path.
3. Stale candidate is marked disqualified and excluded by default views.
4. Flat task payload is rejected; nested payload is accepted.
5. `runner.output` base64 chunk handling respects decode and safety guards.
6. UI smoke verifies task board and task detail status rendering from server fields only.

---

## Review Output Template

Use one entry per finding.

```md
### Finding: <short title>
- Severity: P0 must-fix | P1 should-fix | P2 note
- Impacted file/section: <doc path + heading>
- Risk: <what can break or drift>
- Suggested fix: <specific document-level change>
- Blocking status: Blocking | Non-blocking
```

---

## Exit criteria for prebug review

- All `P0 must-fix` items are `PASS` or have approved mitigation.
- All failed `P1 should-fix` items are captured with owners and follow-up targets.
- No unresolved contradictions remain across v6/v4/finale/checklist docs.
