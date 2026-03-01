# TRACE Phase 0 Human QA Runbook

Date: 2026-03-01

## Goal
Validate the real (non-mocked) browser smoke path on one shared TRACE server/root:
1. Auth check in browser.
2. Agent run trigger + polling.
3. Report retrieval and in-browser rendering.

## Prerequisites
- `tmux` installed.
- Codex CLI installed and logged in (`codex login status`).
- One TRACE server process for the chosen `TRACE_ROOT`.
- `pnpm` installed.

## Environment
Use one shell for server:
```bash
TRACE_SERVER_ADDR=127.0.0.1:18086 \
TRACE_ROOT=/tmp/trace-web-smoke \
TRACE_CODEX_AUTH_POLICY=required \
cargo run -p trace-server
```

Use a second shell for web:
```bash
pnpm --dir web install
pnpm --dir web exec playwright install --with-deps chromium
VITE_TRACE_API_BASE_URL=http://127.0.0.1:18086 pnpm --dir web dev --host 127.0.0.1 --port 4173
```

Use a third shell for tmux helper:
```bash
scripts/trace-smoke-tmux.sh --session trace-smoke start
scripts/trace-smoke-tmux.sh --session trace-smoke validate-target trace-smoke:lanes
```

## Human QA Steps
1. Open `http://127.0.0.1:4173`.
2. In UI, click `Check Codex Auth`.
   - Expect `policy=required`, `available=true`, `logged_in=true`.
3. In UI, click `Run Agents`.
   - Expect `run_id` appears.
   - Expect status reaches terminal (`succeeded` or `failed`) via auto-poll.
4. In UI, click `View Latest Report`.
   - Expect report metadata line (`report_id`, `generated_at`).
   - Expect model summary table rows render.
5. Save smoke/report artifacts:
```bash
RUN_ID="<replace-with-ui-run-id>"
curl -sS "http://127.0.0.1:18086/agent/runs/$RUN_ID" | tee /tmp/trace-smoke-run.json | jq .
curl -sS "http://127.0.0.1:18086/reports?limit=1" | tee /tmp/trace-reports-latest.json | jq .
```
6. Append run results to `docs/PHASE0_SIGNOFF.md`.

## Negative/Failure Checks
1. Invalid target preflight:
   - In UI smoke target field set a bad target (example: `trace-smoke:missing`).
   - Click `Run Agents`.
   - Expect actionable error mentioning `validate-target`.
2. Missing session preflight:
   - Stop session (`scripts/trace-smoke-tmux.sh --session trace-smoke stop`).
   - Click `Run Agents`.
   - Expect actionable error mentioning `status` preflight/session.

## Pass Criteria
- Browser path works end-to-end without filesystem browsing from UI.
- Error states are understandable and actionable.
- Artifacts are captured and linked in `docs/PHASE0_SIGNOFF.md`.
