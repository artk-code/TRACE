# TRACE Phase 0 Finale Plan v1

Date: 2026-02-28  
Depends on: `AGENTS_v6.md`, `FRONTEND_PLAN_v4.md`  
Status: **Execution blueprint for implementation handoff**

## Summary
This is the single-source implementation blueprint for Phase 0. It defines scope, order of work, dependencies, blocking criteria, and acceptance gates required to move from docs-only state to a publishable v0.1 backbone with testing frameworks in place.

---

## Scope

### In scope
- Repository bootstrap for Rust + TypeScript monorepo.
- Backend skeleton for canonical event log, replay gate, lease-sensitive guarding, and candidate eligibility filtering.
- Frontend scaffold aligned to nested task contract and server-authoritative status.
- Testing framework setup:
  - backend unit + integration + API contract tests
  - frontend unit + component tests
  - API E2E + minimal UI smoke
- CI/release baseline for open-source v0.1 artifacts.

### Out of scope
- Full hosted deployment infrastructure.
- Broad end-user feature completeness beyond contract-aligned skeletons.
- Non-critical UI polish and large-scale performance optimization.

---

## Locked public interfaces and contracts

1. `TraceEvent` persisted envelope with required `global_seq`.
2. `NewTraceEvent` producer envelope with `global_seq: None` pre-persist.
3. `TaskResponse` nested canonical shape only.
4. Candidate disqualification semantics with `disqualified_reason=stale_epoch`.
5. `runner.output` payload with `encoding: utf8 | base64`.
6. Phase 0 REST endpoint set:
   - `GET /tasks`
   - `GET /tasks/:task_id`
   - `GET /tasks/:task_id/timeline`
   - `GET /runs/:run_id/timeline`
   - `GET /tasks/:task_id/candidates?include_disqualified=false`
   - `GET /runs/:run_id/output`

---

## Milestones

### M0.1 Workspace ready
- Rust workspace and frontend package scaffolds exist.
- Local typecheck/build/lint commands run.
- CI executes baseline jobs on pull requests.

### M0.2 Contracts enforced
- Canonical types and response envelopes implemented.
- Contract guards/schemas prevent flat-vs-nested drift.
- Contract tests validate public payload shapes.

### M0.3 Replay/guard backbone
- Replay-to-tip gate blocks lease-sensitive operations while behind.
- Gate opens only after replay checkpoint reaches current global tip.
- Stale epoch candidates are classified and filtered by default.

### M0.4 Publishable v0.1 baseline
- API E2E and UI smoke tests pass in CI.
- Release workflow builds artifacts + checksums.
- README run/test/release instructions are present and accurate.

---

## Owner-ready step order (implementation sequence)

1. Bootstrap repository skeleton.
   - Output: workspace/package manifests, toolchain locks, baseline scripts.
   - Dependency: none.
   - Blocking criteria: build commands cannot run consistently across environments.
2. Implement canonical backend types and event persistence API.
   - Output: `NewTraceEvent`, `TraceEvent`, append path with `global_seq` assignment.
   - Dependency: Step 1.
   - Blocking criteria: sequence ordering or envelope invariants not enforceable in tests.
3. Implement replay index + lease-sensitive gate wiring.
   - Output: startup replay contract, checkpoint verification, guarded claim path.
   - Dependency: Step 2.
   - Blocking criteria: guard path not deterministically blocked/opened by replay state.
4. Implement candidate eligibility/disqualification normalization path.
   - Output: stale epoch classification with `disqualified_reason=stale_epoch`.
   - Dependency: Step 3.
   - Blocking criteria: status/compare views leak disqualified candidates by default.
5. Implement API endpoint skeletons with contract-accurate responses.
   - Output: all Phase 0 endpoints return schema-compliant payloads.
   - Dependency: Steps 2-4.
   - Blocking criteria: contract tests fail on shape/nullability.
6. Scaffold frontend package and runtime schema guard layer.
   - Output: typed API client surface and guard wrappers.
   - Dependency: Step 5.
   - Blocking criteria: frontend can render without passing runtime guard validation.
7. Build minimal UI surfaces for task/status/candidate/output flows.
   - Output: task board/detail + candidate list toggle + output panel skeleton.
   - Dependency: Step 6.
   - Blocking criteria: UI computes lifecycle state client-side.
8. Implement test frameworks and baseline suites.
   - Output: unit/integration/contract/E2E/smoke coverage with stable commands.
   - Dependency: Steps 2-7.
   - Blocking criteria: required suites flaky or non-deterministic in CI.
9. Wire CI and release workflow.
   - Output: PR gates and tagged v0.1 artifact pipeline.
   - Dependency: Steps 1-8.
   - Blocking criteria: artifacts cannot be reproduced from tagged source.

---

## Required test scenarios

1. Replay gate blocks lease-sensitive operations when behind tip.
2. Replay completion enables claim APIs and guard path.
3. Stale candidate is marked disqualified and excluded by default views.
4. Flat task payload is rejected while nested payload is accepted.
5. `runner.output` base64 chunk handling respects decode and safety limits.
6. UI smoke verifies task board and task detail status rendering from server fields only.

---

## Definition of Ready to Implement

- [ ] Team confirms `AGENTS_v6.md` and `FRONTEND_PLAN_v4.md` are canonical.
- [ ] No conflicting contract examples remain in repo docs.
- [ ] Milestones M0.1-M0.4 have named owners and target windows.
- [ ] Test scenario names are mapped to executable test files/commands.
- [ ] CI required checks are agreed as merge blockers.
- [ ] Release artifact scope (CLI + local API) is accepted for v0.1.

---

## Assumptions and defaults

1. Repository remains docs-first until coding starts from this plan.
2. Historical files (`AGENTS_v5.md`, `FRONTEND_PLAN_v3.md`) are retained unchanged.
3. Default stack remains Rust backend + TypeScript frontend monorepo.
4. Quality gate profile is balanced: strong contract and integration coverage plus minimal UI smoke.
