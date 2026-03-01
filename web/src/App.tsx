import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import type { BenchmarkReport, CodexAuthStatus, SmokeRunResponse, TmuxCommandResponse } from "./contracts";
import {
  fetchCodexAuthStatus,
  fetchCandidates,
  fetchReport,
  fetchReports,
  fetchRunOutput,
  fetchSmokeRun,
  fetchTasks,
  postSmokeRun,
  postTmuxAddLane,
  postTmuxAddPane,
  postTmuxStart,
  postTmuxStatus,
  postTmuxStop,
} from "./api";
import { decodeOutputChunk, filterCandidates } from "./guards";

function optionalValue(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

function optionalPositiveInt(value: string): number | undefined {
  const trimmed = value.trim();
  if (trimmed === "") {
    return undefined;
  }
  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return undefined;
  }
  return parsed;
}

function parseProfilesInput(value: string): string[] | undefined {
  const profiles = value
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part !== "");
  return profiles.length > 0 ? profiles : undefined;
}

function isSmokeTerminal(status: SmokeRunResponse["status"]): boolean {
  return status === "succeeded" || status === "failed";
}

export default function App() {
  const [includeDisqualified, setIncludeDisqualified] = useState(false);
  const [session, setSession] = useState("trace-smoke");
  const [traceRoot, setTraceRoot] = useState("/tmp/trace-web-smoke");
  const [serverAddr, setServerAddr] = useState("127.0.0.1:18086");
  const [laneName, setLaneName] = useState("codex4");
  const [laneProfile, setLaneProfile] = useState("high");
  const [laneMode, setLaneMode] = useState("interactive");
  const [laneWaitForRunner, setLaneWaitForRunner] = useState(true);
  const [laneRunnerTimeoutSec, setLaneRunnerTimeoutSec] = useState("180");
  const [paneLaneName, setPaneLaneName] = useState("codex5");
  const [paneProfile, setPaneProfile] = useState("flash");
  const [paneMode, setPaneMode] = useState("interactive");
  const [paneWaitForRunner, setPaneWaitForRunner] = useState(true);
  const [paneRunnerTimeoutSec, setPaneRunnerTimeoutSec] = useState("180");
  const [paneTarget, setPaneTarget] = useState("");
  const [smokeTarget, setSmokeTarget] = useState("");
  const [smokeProfiles, setSmokeProfiles] = useState("flash,high,extra");
  const [smokeRunnerTimeoutSec, setSmokeRunnerTimeoutSec] = useState("180");
  const [smokeReportId, setSmokeReportId] = useState("");
  const [smokeBusy, setSmokeBusy] = useState(false);
  const [smokeError, setSmokeError] = useState<string | null>(null);
  const [smokeRun, setSmokeRun] = useState<SmokeRunResponse | null>(null);
  const [smokePollTick, setSmokePollTick] = useState(0);
  const [reportBusy, setReportBusy] = useState(false);
  const [reportError, setReportError] = useState<string | null>(null);
  const [latestReport, setLatestReport] = useState<BenchmarkReport | null>(null);
  const [latestReportSource, setLatestReportSource] = useState<string | null>(null);
  const smokePollAttemptRef = useRef(0);
  const smokePollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [codexAuthStatus, setCodexAuthStatus] = useState<CodexAuthStatus | null>(null);
  const [codexAuthBusy, setCodexAuthBusy] = useState(false);
  const [codexAuthError, setCodexAuthError] = useState<string | null>(null);
  const [orchestrationBusy, setOrchestrationBusy] = useState<string | null>(null);
  const [orchestrationError, setOrchestrationError] = useState<string | null>(null);
  const [orchestrationResult, setOrchestrationResult] = useState<TmuxCommandResponse | null>(null);

  const tasksQuery = useQuery({
    queryKey: ["tasks"],
    queryFn: fetchTasks,
  });

  const primaryTask = tasksQuery.data?.[0] ?? null;
  const taskId = primaryTask?.task.task_id;

  const candidatesQuery = useQuery({
    queryKey: ["candidates", taskId, includeDisqualified],
    queryFn: () => fetchCandidates(taskId as string, includeDisqualified),
    enabled: Boolean(taskId),
  });

  const visibleCandidates = useMemo(
    () => filterCandidates(candidatesQuery.data ?? [], includeDisqualified),
    [candidatesQuery.data, includeDisqualified],
  );

  const runId = visibleCandidates[0]?.run_id;

  const outputQuery = useQuery({
    queryKey: ["output", runId],
    queryFn: () => fetchRunOutput(runId as string),
    enabled: Boolean(runId),
  });

  const outputText = useMemo(() => {
    if (!outputQuery.data) {
      return "";
    }

    return outputQuery.data.map((chunk) => decodeOutputChunk(chunk)).join("\n");
  }, [outputQuery.data]);

  const defaultTmuxTarget = useMemo(() => {
    const normalized = optionalValue(session) ?? "trace-smoke";
    return `${normalized}:lanes`;
  }, [session]);

  async function runOrchestrationAction(
    actionName: string,
    action: () => Promise<TmuxCommandResponse>,
  ): Promise<void> {
    setOrchestrationBusy(actionName);
    setOrchestrationError(null);
    try {
      const result = await action();
      setOrchestrationResult(result);
    } catch (error) {
      setOrchestrationError((error as Error).message);
    } finally {
      setOrchestrationBusy(null);
    }
  }

  async function refreshCodexStatus(): Promise<CodexAuthStatus | null> {
    setCodexAuthBusy(true);
    setCodexAuthError(null);
    try {
      const status = await fetchCodexAuthStatus();
      setCodexAuthStatus(status);
      return status;
    } catch (error) {
      setCodexAuthStatus(null);
      setCodexAuthError((error as Error).message);
      return null;
    } finally {
      setCodexAuthBusy(false);
    }
  }

  async function ensureCodexAuthPreflight(actionName: string): Promise<void> {
    const status = await refreshCodexStatus();
    if (!status) {
      throw new Error(`Unable to verify Codex auth before ${actionName}`);
    }
    if (status.policy === "optional") {
      return;
    }
    if (!status.available) {
      throw new Error(
        `Codex CLI not available. Install Codex CLI or set TRACE_CODEX_BIN. stderr=${status.stderr}`,
      );
    }
    if (!status.logged_in) {
      const command = status.login_commands[0] ?? "codex login";
      throw new Error(`Codex auth required before ${actionName}. Run: ${command}`);
    }
  }

  async function refreshSmokeRunStatus(explicitRunId?: string): Promise<void> {
    const runId = explicitRunId ?? smokeRun?.run_id;
    if (!runId) {
      setSmokeError("No smoke run id is available yet");
      return;
    }

    setSmokeBusy(true);
    setSmokeError(null);
    try {
      const next = await fetchSmokeRun(runId);
      setSmokeRun(next);
    } catch (error) {
      setSmokeError((error as Error).message);
    } finally {
      setSmokeBusy(false);
    }
  }

  async function runSmokeWorkflow(): Promise<void> {
    setSmokeBusy(true);
    setSmokeError(null);
    try {
      await ensureCodexAuthPreflight("smoke-run");
      const started = await postSmokeRun({
        session: optionalValue(session),
        target: optionalValue(smokeTarget) ?? defaultTmuxTarget,
        profiles: parseProfilesInput(smokeProfiles),
        runner_timeout_sec: optionalPositiveInt(smokeRunnerTimeoutSec),
        report_id: optionalValue(smokeReportId),
      });
      smokePollAttemptRef.current = 0;
      setSmokePollTick(0);
      setSmokeRun(started);
    } catch (error) {
      setSmokeError((error as Error).message);
    } finally {
      setSmokeBusy(false);
    }
  }

  async function viewLatestReport(): Promise<void> {
    setReportBusy(true);
    setReportError(null);

    try {
      let reportId = smokeRun?.report_id ?? undefined;
      let source = "smoke run";

      if (!reportId) {
        const list = await fetchReports(1);
        reportId = list.reports[0]?.report_id;
        source = "reports list";
      }

      if (!reportId) {
        setLatestReport(null);
        setLatestReportSource(null);
        setReportError("No reports are available yet");
        return;
      }

      const report = await fetchReport(reportId);
      setLatestReport(report);
      setLatestReportSource(source);
    } catch (error) {
      setLatestReport(null);
      setLatestReportSource(null);
      setReportError((error as Error).message);
    } finally {
      setReportBusy(false);
    }
  }

  useEffect(() => {
    if (smokePollTimerRef.current) {
      clearTimeout(smokePollTimerRef.current);
      smokePollTimerRef.current = null;
    }

    if (!smokeRun || isSmokeTerminal(smokeRun.status)) {
      smokePollAttemptRef.current = 0;
      return;
    }

    const runId = smokeRun.run_id;
    const delaySec = Math.min(1 + smokePollAttemptRef.current, 3);
    smokePollTimerRef.current = setTimeout(async () => {
      smokePollAttemptRef.current += 1;
      try {
        const next = await fetchSmokeRun(runId);
        setSmokeError(null);
        setSmokeRun(next);
      } catch (error) {
        setSmokeError((error as Error).message);
        setSmokePollTick((value) => value + 1);
      }
    }, delaySec * 1000);

    return () => {
      if (smokePollTimerRef.current) {
        clearTimeout(smokePollTimerRef.current);
        smokePollTimerRef.current = null;
      }
    };
  }, [smokeRun, smokePollTick]);

  return (
    <main style={{ fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", padding: 20 }}>
      <h1>TRACE Phase 0 Scaffold</h1>

      <section>
        <h2>Orchestration</h2>
        <p>Control tmux session lifecycle through TRACE API orchestration routes.</p>
        <p>
          <button onClick={() => void refreshCodexStatus()} disabled={codexAuthBusy}>
            Check Codex Auth
          </button>
        </p>
        {codexAuthBusy ? <p>Checking Codex auth...</p> : null}
        {codexAuthError ? <p>Codex auth check failed: {codexAuthError}</p> : null}
        {codexAuthStatus ? <p>Auth policy: {codexAuthStatus.policy}</p> : null}
        {codexAuthStatus ? <pre>{JSON.stringify(codexAuthStatus, null, 2)}</pre> : null}
        {codexAuthStatus?.requires_login ? (
          <p>Run one of: {codexAuthStatus.login_commands.join(" | ")}</p>
        ) : null}
        <fieldset disabled={Boolean(orchestrationBusy)}>
          <label>
            Session:
            <input value={session} onChange={(event) => setSession(event.target.value)} />
          </label>
          <label>
            Trace Root:
            <input value={traceRoot} onChange={(event) => setTraceRoot(event.target.value)} />
          </label>
          <label>
            Server Addr:
            <input value={serverAddr} onChange={(event) => setServerAddr(event.target.value)} />
          </label>
          <label>
            Add-Lane Name:
            <input value={laneName} onChange={(event) => setLaneName(event.target.value)} />
          </label>
          <label>
            Add-Lane Profile:
            <input value={laneProfile} onChange={(event) => setLaneProfile(event.target.value)} />
          </label>
          <label>
            Add-Lane Mode:
            <select value={laneMode} onChange={(event) => setLaneMode(event.target.value)}>
              <option value="interactive">interactive</option>
              <option value="runner">runner</option>
            </select>
          </label>
          <label>
            Add-Lane Wait:
            <input
              type="checkbox"
              checked={laneWaitForRunner}
              disabled={laneMode !== "runner"}
              onChange={(event) => setLaneWaitForRunner(event.target.checked)}
            />
          </label>
          <label>
            Add-Lane Timeout (s):
            <input
              value={laneRunnerTimeoutSec}
              disabled={laneMode !== "runner"}
              onChange={(event) => setLaneRunnerTimeoutSec(event.target.value)}
            />
          </label>
          <label>
            Add-Pane Name:
            <input value={paneLaneName} onChange={(event) => setPaneLaneName(event.target.value)} />
          </label>
          <label>
            Add-Pane Profile:
            <input value={paneProfile} onChange={(event) => setPaneProfile(event.target.value)} />
          </label>
          <label>
            Add-Pane Mode:
            <select value={paneMode} onChange={(event) => setPaneMode(event.target.value)}>
              <option value="interactive">interactive</option>
              <option value="runner">runner</option>
            </select>
          </label>
          <label>
            Add-Pane Wait:
            <input
              type="checkbox"
              checked={paneWaitForRunner}
              disabled={paneMode !== "runner"}
              onChange={(event) => setPaneWaitForRunner(event.target.checked)}
            />
          </label>
          <label>
            Add-Pane Timeout (s):
            <input
              value={paneRunnerTimeoutSec}
              disabled={paneMode !== "runner"}
              onChange={(event) => setPaneRunnerTimeoutSec(event.target.value)}
            />
          </label>
          <label>
            Add-Pane Target:
            <input
              value={paneTarget}
              placeholder={defaultTmuxTarget}
              onChange={(event) => setPaneTarget(event.target.value)}
            />
          </label>
        </fieldset>
        <p>
          <button
            onClick={() =>
              runOrchestrationAction("start", () =>
                postTmuxStart({
                  session: optionalValue(session),
                  trace_root: optionalValue(traceRoot),
                  addr: optionalValue(serverAddr),
                }),
              )
            }
            disabled={Boolean(orchestrationBusy)}
          >
            Start Session
          </button>{" "}
          <button
            onClick={() =>
              runOrchestrationAction("status", () =>
                postTmuxStatus({
                  session: optionalValue(session),
                }),
              )
            }
            disabled={Boolean(orchestrationBusy)}
          >
            Status
          </button>{" "}
          <button
            onClick={() =>
              runOrchestrationAction("add-lane", async () => {
                await ensureCodexAuthPreflight("add-lane");
                return postTmuxAddLane({
                  session: optionalValue(session),
                  lane_name: laneName.trim(),
                  profile: optionalValue(laneProfile),
                  mode: laneMode === "interactive" ? undefined : laneMode,
                  wait_for_runner: laneMode === "runner" ? laneWaitForRunner : undefined,
                  runner_timeout_sec:
                    laneMode === "runner" ? optionalPositiveInt(laneRunnerTimeoutSec) : undefined,
                });
              })
            }
            disabled={Boolean(orchestrationBusy) || codexAuthBusy}
          >
            Add Lane
          </button>{" "}
          <button
            onClick={() =>
              runOrchestrationAction("add-pane", async () => {
                await ensureCodexAuthPreflight("add-pane");
                return postTmuxAddPane({
                  session: optionalValue(session),
                  lane_name: paneLaneName.trim(),
                  profile: optionalValue(paneProfile),
                  target: optionalValue(paneTarget) ?? defaultTmuxTarget,
                  mode: paneMode === "interactive" ? undefined : paneMode,
                  wait_for_runner: paneMode === "runner" ? paneWaitForRunner : undefined,
                  runner_timeout_sec:
                    paneMode === "runner" ? optionalPositiveInt(paneRunnerTimeoutSec) : undefined,
                });
              })
            }
            disabled={Boolean(orchestrationBusy) || codexAuthBusy}
          >
            Add Pane
          </button>{" "}
          <button
            onClick={() =>
              runOrchestrationAction("stop", () =>
                postTmuxStop({
                  session: optionalValue(session),
                }),
              )
            }
            disabled={Boolean(orchestrationBusy)}
          >
            Stop Session
          </button>
        </p>
        {orchestrationBusy ? <p>Running action: {orchestrationBusy}</p> : null}
        {orchestrationError ? <p>Orchestration failed: {orchestrationError}</p> : null}
        {orchestrationResult ? (
          <pre>{JSON.stringify(orchestrationResult, null, 2)}</pre>
        ) : (
          <pre>No orchestration command executed yet.</pre>
        )}
      </section>

      <section>
        <h2>Smoke Workflow</h2>
        <p>Run multi-lane smoke workflow and poll run status.</p>
        <fieldset disabled={smokeBusy || Boolean(orchestrationBusy)}>
          <label>
            Session:
            <input value={session} onChange={(event) => setSession(event.target.value)} />
          </label>
          <label>
            Target:
            <input
              value={smokeTarget}
              placeholder={defaultTmuxTarget}
              onChange={(event) => setSmokeTarget(event.target.value)}
            />
          </label>
          <label>
            Profiles (comma-separated):
            <input
              value={smokeProfiles}
              onChange={(event) => setSmokeProfiles(event.target.value)}
            />
          </label>
          <label>
            Runner Timeout (s):
            <input
              value={smokeRunnerTimeoutSec}
              onChange={(event) => setSmokeRunnerTimeoutSec(event.target.value)}
            />
          </label>
          <label>
            Report ID (optional):
            <input value={smokeReportId} onChange={(event) => setSmokeReportId(event.target.value)} />
          </label>
        </fieldset>
        <p>
          <button onClick={() => void runSmokeWorkflow()} disabled={smokeBusy || codexAuthBusy}>
            Run Smoke
          </button>{" "}
          <button
            onClick={() => void refreshSmokeRunStatus()}
            disabled={smokeBusy || !smokeRun?.run_id}
          >
            Refresh Status
          </button>{" "}
          <button onClick={() => void viewLatestReport()} disabled={reportBusy}>
            View Latest Report
          </button>
        </p>
        {smokeBusy ? <p>Running smoke action...</p> : null}
        {smokeRun && !isSmokeTerminal(smokeRun.status) ? (
          <p>Auto-polling status while run is active.</p>
        ) : null}
        {smokeError ? <p>Smoke workflow failed: {smokeError}</p> : null}
        {smokeRun ? (
          <>
            <p>
              run_id={smokeRun.run_id} | status={smokeRun.status} | step={smokeRun.current_step}
            </p>
            {smokeRun.error ? <p>run error: {smokeRun.error}</p> : null}
            {smokeRun.report_id ? <p>report_id={smokeRun.report_id}</p> : null}
            {smokeRun.summary ? (
              <pre>{JSON.stringify(smokeRun.summary, null, 2)}</pre>
            ) : (
              <pre>No benchmark summary available yet.</pre>
            )}
          </>
        ) : (
          <pre>No smoke run started yet.</pre>
        )}
      </section>

      <section>
        <h2>Latest Report</h2>
        <p>Fetch the newest benchmark report and render model-level summary.</p>
        {reportBusy ? <p>Loading report...</p> : null}
        {reportError ? <p>Report retrieval failed: {reportError}</p> : null}
        {latestReport ? (
          <>
            <p>
              report_id={latestReport.report_id} | generated_at={latestReport.generated_at} | source=
              {latestReportSource ?? "n/a"}
            </p>
            <p>
              total_tasks={latestReport.total_tasks} | total_runs={latestReport.total_runs} | total_events=
              {latestReport.total_events}
            </p>
            <table>
              <thead>
                <tr>
                  <th>model_key</th>
                  <th>provider</th>
                  <th>model</th>
                  <th>profile</th>
                  <th>runs</th>
                  <th>pass</th>
                  <th>fail</th>
                  <th>candidates</th>
                  <th>eligible</th>
                  <th>disqualified</th>
                  <th>output_bytes</th>
                  <th>avg_duration_ms</th>
                </tr>
              </thead>
              <tbody>
                {latestReport.models.map((modelSummary) => (
                  <tr key={modelSummary.model_key}>
                    <td>{modelSummary.model_key}</td>
                    <td>{modelSummary.provider ?? "-"}</td>
                    <td>{modelSummary.model ?? "-"}</td>
                    <td>{modelSummary.profile ?? "-"}</td>
                    <td>{modelSummary.runs}</td>
                    <td>{modelSummary.pass_count}</td>
                    <td>{modelSummary.fail_count}</td>
                    <td>{modelSummary.candidate_total}</td>
                    <td>{modelSummary.candidate_eligible}</td>
                    <td>{modelSummary.candidate_disqualified}</td>
                    <td>{modelSummary.output_bytes}</td>
                    <td>{modelSummary.avg_duration_ms ?? "-"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        ) : (
          <pre>No report loaded yet.</pre>
        )}
      </section>

      <section>
        <h2>Tasks</h2>
        {tasksQuery.isLoading ? <p>Loading tasks...</p> : null}
        {tasksQuery.error ? <p>Task fetch failed: {(tasksQuery.error as Error).message}</p> : null}
        <ul>
          {(tasksQuery.data ?? []).map((task) => (
            <li key={task.task.task_id}>
              {task.task.task_id} | {task.status} | {task.task.title}
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2>Candidates</h2>
        <label>
          <input
            type="checkbox"
            checked={includeDisqualified}
            onChange={(event) => setIncludeDisqualified(event.target.checked)}
          />
          Show stale/disqualified
        </label>
        {candidatesQuery.isLoading ? <p>Loading candidates...</p> : null}
        {candidatesQuery.error ? (
          <p>Candidate fetch failed: {(candidatesQuery.error as Error).message}</p>
        ) : null}
        <ul>
          {visibleCandidates.map((candidate) => (
            <li key={candidate.candidate_id}>
              {candidate.candidate_id} | run={candidate.run_id} | eligible={String(candidate.eligible)}
              {candidate.disqualified_reason ? ` | reason=${candidate.disqualified_reason}` : ""}
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h2>Run Output</h2>
        {outputQuery.isLoading ? <p>Loading output...</p> : null}
        {outputQuery.error ? <p>Output fetch failed: {(outputQuery.error as Error).message}</p> : null}
        <pre>{outputText}</pre>
      </section>
    </main>
  );
}
