# TRACE
TRACE is a *local-first* harness that binds agent work to tasks, records immutable traces, versions outputs as ChangeSets, evaluates candidates deterministically, and supports recombination/stacking to pick a winner.

## Planning Docs
- [AGENTS_v6.md](/Users/artk/Documents/GitHub/TRACE/AGENTS_v6.md)
- [FRONTEND_PLAN_v4.md](/Users/artk/Documents/GitHub/TRACE/FRONTEND_PLAN_v4.md)
- [PHASE0_FINALE_PLAN_v1.md](/Users/artk/Documents/GitHub/TRACE/PHASE0_FINALE_PLAN_v1.md)
- [PREBUG_CHECKLIST_v1.md](/Users/artk/Documents/GitHub/TRACE/PREBUG_CHECKLIST_v1.md)

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

## Phase 0 Status
- Monorepo scaffold is now in place (Rust + TypeScript workspace).
- Contract-first backend types, replay gate, candidate filtering, and API skeleton code have initial implementation.
- Frontend runtime guards and starter views are scaffolded with initial unit tests.
