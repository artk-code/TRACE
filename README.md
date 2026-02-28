# TRACE
TRACE is a *local-first* harness that binds agent work to tasks, records immutable traces, versions outputs as ChangeSets, evaluates candidates deterministically, and supports recombination/stacking to pick a winner.

## Active Planning Docs
- [AGENTS.md](/Users/artk/Documents/GitHub/TRACE/AGENTS.md)
- [BUILD_SEQUENCE_PLAN_v2.md](/Users/artk/Documents/GitHub/TRACE/BUILD_SEQUENCE_PLAN_v2.md)
- [SMOKETEST_EVAL_PLAN_v1.md](/Users/artk/Documents/GitHub/TRACE/SMOKETEST_EVAL_PLAN_v1.md)
- [FRONTEND_PLAN_v4.md](/Users/artk/Documents/GitHub/TRACE/FRONTEND_PLAN_v4.md)
- [FRONTEND_PLAN_v3.md](/Users/artk/Documents/GitHub/TRACE/FRONTEND_PLAN_v3.md)

## Archived Planning Docs
- [archive/phase0_docs](/Users/artk/Documents/GitHub/TRACE/archive/phase0_docs)

## Workspace Layout
- Rust workspace crates live in `/Users/artk/Documents/GitHub/TRACE/crates`.
- Frontend package lives in `/Users/artk/Documents/GitHub/TRACE/web`.
- Canonical event log path is `.trace/events/events.jsonl`.

## Build + Test
1. Backend tests:
```bash
cargo test --workspace
```
2. Web tests:
```bash
pnpm --dir web install
pnpm --dir web test
```

## Current Status
- Monorepo scaffold is in place (Rust + TypeScript workspace).
- Read-side API projections from canonical event log are implemented.
- Next milestone is multi-writer support and benchmark smoke flow (see active plans).
