# TRACE Phase 0 Sign-Off

Date: 2026-03-01  
Status: Pending human QA evidence

## Scope
Phase 0 ("Super Smoketest") is complete when a browser user can run smoke end-to-end on one TRACE server/root, retrieve benchmark reports over HTTP, and pass a CI-gated browser smoke test.

## Exit Criteria Evidence Map
| Exit criterion | Automated evidence | Human QA evidence |
| --- | --- | --- |
| Browser UI can trigger and observe full smoke run | `web/src/App.tsx`, `web/tests/phase0-smoke.spec.ts` | Run checklist steps 1-6 in `docs/PHASE0_HUMAN_QA.md` |
| Concurrent writes do not corrupt `global_seq` | `crates/trace-store/src/lib.rs:test_concurrent_appends_produce_unique_contiguous_sequences` | Record run ids + report id from a real 3-lane run |
| Benchmark report retrievable/renderable in browser | `crates/trace-server/src/lib.rs` report API tests + UI report table | Capture `GET /reports` response and UI screenshot |
| Browser E2E smoke is stable and CI-gated | `.github/workflows/ci.yml` web job runs `pnpm test:e2e` | Confirm CI green on merge commit |

## Run Record Template
Copy this section and fill one block per sign-off run.

### Sign-Off Run <N>
- Date:
- Operator:
- Host OS:
- TRACE commit:
- TRACE_ROOT:
- TRACE_SERVER_ADDR:
- tmux session:
- smoke run_id:
- report_id:
- Smoke terminal status (`succeeded|failed`):
- Notes:

Artifacts:
- `/tmp/trace-smoke-run.json`
- `/tmp/trace-reports-latest.json`
- Optional screenshot path(s)

### Sign-Off Run 1
- Date: 2026-03-01T09:57:48Z
- Operator: Codex agent
- Host OS: Darwin 25.2.0
- TRACE commit: `d1801ca` (plus local completion-candidate changes)
- TRACE_ROOT: `/Users/artk/Documents/GitHub/TRACE/.trace-smoke`
- TRACE_SERVER_ADDR: `127.0.0.1:18080`
- tmux session: `trace-smoke`
- smoke run_id: `smoke-1772359038982-1`
- report_id: `smoke-smoke-1772359038982-1`
- Smoke terminal status (`succeeded|failed`): `succeeded`
- Notes: Real tmux-backed smoke run completed and report was generated/retrieved via live server APIs. Browser UI interaction still requires explicit human-run confirmation per runbook.

Artifacts:
- `/tmp/trace-smoke-run.json`
- `/tmp/trace-reports-latest.json`
- Optional screenshot path(s): not captured in this run

## Final Verdict
- [ ] Human QA run executed against real server + tmux target
- [ ] Artifacts attached and non-empty
- [ ] CI green with `pnpm --dir web test:e2e`
- [ ] All Phase 0 exit criteria satisfied

If all boxes are checked, Phase 0 can be marked complete.
