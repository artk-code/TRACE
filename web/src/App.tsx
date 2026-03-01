import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import type {
  AgentRunResponse,
  BenchmarkReport,
  CodexAuthStatus,
  TmuxCommandResponse,
  TmuxPaneSnapshot,
  TmuxSnapshotResponse,
} from "./contracts";
import {
  fetchCodexAuthStatus,
  fetchCandidates,
  fetchReport,
  fetchReports,
  fetchRunOutput,
  fetchAgentRun,
  fetchTasks,
  postAgentRun,
  postTmuxAddLane,
  postTmuxAddPane,
  postTmuxCapture,
  postTmuxSendKeys,
  postTmuxSnapshot,
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

function isAgentTerminal(status: AgentRunResponse["status"]): boolean {
  return status === "succeeded" || status === "failed";
}

function findPaneById(snapshot: TmuxSnapshotResponse | null, paneId: string): TmuxPaneSnapshot | null {
  if (!snapshot || paneId.trim() === "") {
    return null;
  }
  return snapshot.panes.find((pane) => pane.pane_id === paneId) ?? null;
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
  const [agentTarget, setAgentTarget] = useState("");
  const [agentProfiles, setAgentProfiles] = useState("flash,high,extra");
  const [agentRunnerTimeoutSec, setAgentRunnerTimeoutSec] = useState("180");
  const [agentReportId, setAgentReportId] = useState("");
  const [agentOutputMode, setAgentOutputMode] = useState<"codex" | "scripted">("codex");
  const [agentReasoningEffort, setAgentReasoningEffort] = useState("low");
  const [agentTaskCount, setAgentTaskCount] = useState("1");
  const [agentTaskPrefix, setAgentTaskPrefix] = useState("TASK-SMOKE");
  const [agentInputSource, setAgentInputSource] = useState<"predefined" | "human">("predefined");
  const [agentHumanPrompt, setAgentHumanPrompt] = useState("");
  const [agentBusy, setAgentBusy] = useState(false);
  const [agentError, setAgentError] = useState<string | null>(null);
  const [agentRun, setAgentRun] = useState<AgentRunResponse | null>(null);
  const [agentPollTick, setAgentPollTick] = useState(0);
  const [reportBusy, setReportBusy] = useState(false);
  const [reportError, setReportError] = useState<string | null>(null);
  const [latestReport, setLatestReport] = useState<BenchmarkReport | null>(null);
  const [latestReportSource, setLatestReportSource] = useState<string | null>(null);
  const agentPollAttemptRef = useRef(0);
  const agentPollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [codexAuthStatus, setCodexAuthStatus] = useState<CodexAuthStatus | null>(null);
  const [codexAuthBusy, setCodexAuthBusy] = useState(false);
  const [codexAuthError, setCodexAuthError] = useState<string | null>(null);
  const [orchestrationBusy, setOrchestrationBusy] = useState<string | null>(null);
  const [orchestrationError, setOrchestrationError] = useState<string | null>(null);
  const [orchestrationResult, setOrchestrationResult] = useState<TmuxCommandResponse | null>(null);
  const [tmuxSnapshotBusy, setTmuxSnapshotBusy] = useState(false);
  const [tmuxSnapshotError, setTmuxSnapshotError] = useState<string | null>(null);
  const [tmuxSnapshot, setTmuxSnapshot] = useState<TmuxSnapshotResponse | null>(null);
  const [selectedPaneId, setSelectedPaneId] = useState("");
  const [paneLines, setPaneLines] = useState("200");
  const [paneCaptureBusy, setPaneCaptureBusy] = useState(false);
  const [paneCaptureError, setPaneCaptureError] = useState<string | null>(null);
  const [paneCaptureText, setPaneCaptureText] = useState("");
  const [paneCapturedAt, setPaneCapturedAt] = useState<string | null>(null);
  const [paneInput, setPaneInput] = useState("");
  const [paneInputBusy, setPaneInputBusy] = useState(false);
  const [paneInputError, setPaneInputError] = useState<string | null>(null);
  const [paneInputLastAction, setPaneInputLastAction] = useState<string | null>(null);
  const panePollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  const selectedPane = useMemo(
    () => findPaneById(tmuxSnapshot, selectedPaneId),
    [tmuxSnapshot, selectedPaneId],
  );

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

  async function refreshTmuxSnapshot(): Promise<void> {
    setTmuxSnapshotBusy(true);
    setTmuxSnapshotError(null);
    try {
      const snapshot = await postTmuxSnapshot({
        session: optionalValue(session),
      });
      setTmuxSnapshot(snapshot);
      setSelectedPaneId((current) => {
        if (current !== "" && snapshot.panes.some((pane) => pane.pane_id === current)) {
          return current;
        }
        return snapshot.panes[0]?.pane_id ?? "";
      });
      if (snapshot.panes.length === 0) {
        setPaneCaptureText("");
        setPaneCapturedAt(null);
      }
    } catch (error) {
      setTmuxSnapshotError((error as Error).message);
      setTmuxSnapshot(null);
      setSelectedPaneId("");
      setPaneCaptureText("");
      setPaneCapturedAt(null);
    } finally {
      setTmuxSnapshotBusy(false);
    }
  }

  async function refreshPaneCapture(explicitPane?: TmuxPaneSnapshot | null): Promise<void> {
    const pane = explicitPane ?? selectedPane;
    if (!pane) {
      setPaneCaptureError("Select a pane from the session tree first");
      return;
    }

    setPaneCaptureBusy(true);
    setPaneCaptureError(null);
    try {
      const capture = await postTmuxCapture({
        session: optionalValue(session),
        target: pane.pane_id,
        lines: optionalPositiveInt(paneLines),
      });
      setPaneCaptureText(capture.content);
      setPaneCapturedAt(capture.captured_at);
    } catch (error) {
      setPaneCaptureError((error as Error).message);
    } finally {
      setPaneCaptureBusy(false);
    }
  }

  async function sendPaneInput(options?: {
    text?: string;
    key?: string;
    pressEnter?: boolean;
    clearTextAfterSend?: boolean;
  }): Promise<void> {
    if (!selectedPane) {
      setPaneInputError("Select a pane before sending input");
      return;
    }

    const text = options?.text;
    const key = options?.key;
    const pressEnter = options?.pressEnter ?? false;
    if ((!text || text.trim() === "") && !key && !pressEnter) {
      setPaneInputError("Type input text or choose a shortcut key");
      return;
    }

    setPaneInputBusy(true);
    setPaneInputError(null);
    try {
      await postTmuxSendKeys({
        session: optionalValue(session),
        target: selectedPane.pane_id,
        text,
        key,
        press_enter: pressEnter,
      });
      const actionParts = [
        text && text.trim() !== "" ? `text="${text}"` : null,
        key ? `key=${key}` : null,
        pressEnter ? "enter=true" : null,
      ].filter((part): part is string => Boolean(part));
      setPaneInputLastAction(actionParts.join(" | "));
      if (options?.clearTextAfterSend) {
        setPaneInput("");
      }
      await refreshPaneCapture(selectedPane);
    } catch (error) {
      setPaneInputError((error as Error).message);
    } finally {
      setPaneInputBusy(false);
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

  async function refreshAgentRunStatus(explicitRunId?: string): Promise<void> {
    const runId = explicitRunId ?? agentRun?.run_id;
    if (!runId) {
      setAgentError("No agent run id is available yet");
      return;
    }

    setAgentBusy(true);
    setAgentError(null);
    try {
      const next = await fetchAgentRun(runId);
      setAgentRun(next);
    } catch (error) {
      setAgentError((error as Error).message);
    } finally {
      setAgentBusy(false);
    }
  }

  async function runAgentWorkflow(): Promise<void> {
    setAgentBusy(true);
    setAgentError(null);
    try {
      await ensureCodexAuthPreflight("agent-run");
      const started = await postAgentRun({
        session: optionalValue(session),
        target: optionalValue(agentTarget) ?? defaultTmuxTarget,
        profiles: parseProfilesInput(agentProfiles),
        runner_timeout_sec: optionalPositiveInt(agentRunnerTimeoutSec),
        report_id: optionalValue(agentReportId),
        runner_output_mode: agentOutputMode,
        runner_task_count: optionalPositiveInt(agentTaskCount),
        runner_task_prefix: optionalValue(agentTaskPrefix),
        runner_reasoning_effort:
          agentOutputMode === "codex" ? optionalValue(agentReasoningEffort) : undefined,
        runner_codex_prompt:
          agentOutputMode === "codex" && agentInputSource === "human"
            ? optionalValue(agentHumanPrompt)
            : undefined,
      });
      agentPollAttemptRef.current = 0;
      setAgentPollTick(0);
      setAgentRun(started);
    } catch (error) {
      setAgentError((error as Error).message);
    } finally {
      setAgentBusy(false);
    }
  }

  async function viewLatestReport(): Promise<void> {
    setReportBusy(true);
    setReportError(null);

    try {
      let reportId = agentRun?.report_id ?? undefined;
      let source = "agent run";

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
    if (agentPollTimerRef.current) {
      clearTimeout(agentPollTimerRef.current);
      agentPollTimerRef.current = null;
    }

    if (!agentRun || isAgentTerminal(agentRun.status)) {
      agentPollAttemptRef.current = 0;
      return;
    }

    const runId = agentRun.run_id;
    const delaySec = Math.min(1 + agentPollAttemptRef.current, 3);
    agentPollTimerRef.current = setTimeout(async () => {
      agentPollAttemptRef.current += 1;
      try {
        const next = await fetchAgentRun(runId);
        setAgentError(null);
        setAgentRun(next);
      } catch (error) {
        setAgentError((error as Error).message);
        setAgentPollTick((value) => value + 1);
      }
    }, delaySec * 1000);

    return () => {
      if (agentPollTimerRef.current) {
        clearTimeout(agentPollTimerRef.current);
        agentPollTimerRef.current = null;
      }
    };
  }, [agentRun, agentPollTick]);

  useEffect(() => {
    if (panePollTimerRef.current) {
      clearTimeout(panePollTimerRef.current);
      panePollTimerRef.current = null;
    }

    if (!selectedPane) {
      setPaneCaptureText("");
      setPaneCapturedAt(null);
      return;
    }

    let canceled = false;
    const lines = optionalPositiveInt(paneLines);

    const poll = async (): Promise<void> => {
      if (canceled) {
        return;
      }

      setPaneCaptureBusy(true);
      try {
        const capture = await postTmuxCapture({
          session: optionalValue(session),
          target: selectedPane.pane_id,
          lines,
        });
        if (!canceled) {
          setPaneCaptureError(null);
          setPaneCaptureText(capture.content);
          setPaneCapturedAt(capture.captured_at);
        }
      } catch (error) {
        if (!canceled) {
          setPaneCaptureError((error as Error).message);
        }
      } finally {
        if (!canceled) {
          setPaneCaptureBusy(false);
          panePollTimerRef.current = setTimeout(() => {
            void poll();
          }, 2000);
        }
      }
    };

    void poll();

    return () => {
      canceled = true;
      if (panePollTimerRef.current) {
        clearTimeout(panePollTimerRef.current);
        panePollTimerRef.current = null;
      }
    };
  }, [selectedPane, paneLines, session]);

  useEffect(() => {
    setPaneInputError(null);
    setPaneInputLastAction(null);
  }, [selectedPaneId]);

  return (
    <div className="trace-app-shell">
      <div className="trace-aurora trace-aurora-a" />
      <div className="trace-aurora trace-aurora-b" />
      <main className="trace-app">
        <header className="trace-hero">
          <p className="trace-eyebrow">TRACE Operator Console</p>
          <h1>Multi-Agent Control Panel</h1>
          <p>
            Start lanes, run agents, monitor status, and review benchmark output from one browser
            workspace.
          </p>
        </header>

        <section className="trace-panel trace-panel-wide">
          <h2>Orchestration</h2>
          <p>Control tmux session lifecycle through TRACE API orchestration routes.</p>
          <div className="trace-button-row">
            <button className="trace-btn trace-btn-primary" onClick={() => void refreshCodexStatus()} disabled={codexAuthBusy}>
              Check Codex Auth
            </button>
            <button
              className="trace-btn"
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
            </button>
            <button
              className="trace-btn"
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
            </button>
            <button
              className="trace-btn"
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
          </div>
          {codexAuthBusy ? <p className="trace-note">Checking Codex auth...</p> : null}
          {codexAuthError ? <p className="trace-error">Codex auth check failed: {codexAuthError}</p> : null}
          {codexAuthStatus ? <p className="trace-note">Auth policy: {codexAuthStatus.policy}</p> : null}
          {codexAuthStatus?.requires_login ? (
            <p className="trace-warning">Run one of: {codexAuthStatus.login_commands.join(" | ")}</p>
          ) : null}
          {codexAuthStatus ? <pre className="trace-console">{JSON.stringify(codexAuthStatus, null, 2)}</pre> : null}

          <fieldset className="trace-fieldset" disabled={Boolean(orchestrationBusy)}>
            <div className="trace-fields">
              <label className="trace-field">
                Session:
                <input value={session} onChange={(event) => setSession(event.target.value)} />
              </label>
              <label className="trace-field">
                Trace Root:
                <input value={traceRoot} onChange={(event) => setTraceRoot(event.target.value)} />
              </label>
              <label className="trace-field">
                Server Addr:
                <input value={serverAddr} onChange={(event) => setServerAddr(event.target.value)} />
              </label>
            </div>

            <div className="trace-inline-grid">
              <h3>Add Lane</h3>
              <label className="trace-field">
                Add-Lane Name:
                <input value={laneName} onChange={(event) => setLaneName(event.target.value)} />
              </label>
              <label className="trace-field">
                Add-Lane Profile:
                <input value={laneProfile} onChange={(event) => setLaneProfile(event.target.value)} />
              </label>
              <label className="trace-field">
                Add-Lane Mode:
                <select value={laneMode} onChange={(event) => setLaneMode(event.target.value)}>
                  <option value="interactive">interactive</option>
                  <option value="runner">runner</option>
                </select>
              </label>
              <label className="trace-field trace-checkbox">
                Add-Lane Wait:
                <input
                  type="checkbox"
                  checked={laneWaitForRunner}
                  disabled={laneMode !== "runner"}
                  onChange={(event) => setLaneWaitForRunner(event.target.checked)}
                />
              </label>
              <label className="trace-field">
                Add-Lane Timeout (s):
                <input
                  value={laneRunnerTimeoutSec}
                  disabled={laneMode !== "runner"}
                  onChange={(event) => setLaneRunnerTimeoutSec(event.target.value)}
                />
              </label>
              <button
                className="trace-btn"
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
              </button>
            </div>

            <div className="trace-inline-grid">
              <h3>Add Pane</h3>
              <label className="trace-field">
                Add-Pane Name:
                <input value={paneLaneName} onChange={(event) => setPaneLaneName(event.target.value)} />
              </label>
              <label className="trace-field">
                Add-Pane Profile:
                <input value={paneProfile} onChange={(event) => setPaneProfile(event.target.value)} />
              </label>
              <label className="trace-field">
                Add-Pane Mode:
                <select value={paneMode} onChange={(event) => setPaneMode(event.target.value)}>
                  <option value="interactive">interactive</option>
                  <option value="runner">runner</option>
                </select>
              </label>
              <label className="trace-field trace-checkbox">
                Add-Pane Wait:
                <input
                  type="checkbox"
                  checked={paneWaitForRunner}
                  disabled={paneMode !== "runner"}
                  onChange={(event) => setPaneWaitForRunner(event.target.checked)}
                />
              </label>
              <label className="trace-field">
                Add-Pane Timeout (s):
                <input
                  value={paneRunnerTimeoutSec}
                  disabled={paneMode !== "runner"}
                  onChange={(event) => setPaneRunnerTimeoutSec(event.target.value)}
                />
              </label>
              <label className="trace-field">
                Add-Pane Target:
                <input
                  value={paneTarget}
                  placeholder={defaultTmuxTarget}
                  onChange={(event) => setPaneTarget(event.target.value)}
                />
              </label>
              <button
                className="trace-btn"
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
              </button>
            </div>
          </fieldset>
          {orchestrationBusy ? <p className="trace-note">Running action: {orchestrationBusy}</p> : null}
          {orchestrationError ? <p className="trace-error">Orchestration failed: {orchestrationError}</p> : null}
          {orchestrationResult ? (
            <pre className="trace-console">{JSON.stringify(orchestrationResult, null, 2)}</pre>
          ) : (
            <pre className="trace-console">No orchestration command executed yet.</pre>
          )}
        </section>

        <section className="trace-panel trace-panel-wide">
          <h2>Terminal Workspace</h2>
          <p>Browse tmux windows/panes, stream live output, and send lane input from the browser.</p>
          <div className="trace-button-row">
            <button className="trace-btn trace-btn-primary" onClick={() => void refreshTmuxSnapshot()} disabled={tmuxSnapshotBusy}>
              Load Session Tree
            </button>
            <button
              className="trace-btn"
              onClick={async () => {
                await refreshTmuxSnapshot();
                await refreshPaneCapture();
              }}
              disabled={tmuxSnapshotBusy || paneCaptureBusy}
            >
              Reconnect Stream
            </button>
            <button
              className="trace-btn"
              onClick={() => void refreshPaneCapture()}
              disabled={paneCaptureBusy || !selectedPane || tmuxSnapshotBusy}
            >
              Capture Now
            </button>
          </div>
          <label className="trace-field">
            Capture Lines:
            <input value={paneLines} onChange={(event) => setPaneLines(event.target.value)} />
          </label>
          {tmuxSnapshotBusy ? <p className="trace-note">Loading tmux session tree...</p> : null}
          {tmuxSnapshotError ? <p className="trace-error">Session tree failed: {tmuxSnapshotError}</p> : null}
          {tmuxSnapshot ? (
            <p className="trace-note">
              session={tmuxSnapshot.session} | windows={tmuxSnapshot.windows.length} | panes=
              {tmuxSnapshot.panes.length}
            </p>
          ) : (
            <p className="trace-note">No session snapshot loaded yet.</p>
          )}
          <div className="trace-terminal-grid">
            <aside className="trace-terminal-sidebar">
              <h3>Panes</h3>
              {tmuxSnapshot && tmuxSnapshot.panes.length > 0 ? (
                <div className="trace-pane-list">
                  {tmuxSnapshot.panes.map((pane) => {
                    const paneLabel = `${pane.window_name}.${pane.pane_index} (${pane.pane_id})`;
                    return (
                      <button
                        key={pane.pane_id}
                        className={`trace-pane-btn ${selectedPaneId === pane.pane_id ? "is-selected" : ""}`}
                        onClick={() => setSelectedPaneId(pane.pane_id)}
                        type="button"
                      >
                        <span>{paneLabel}</span>
                        <span>
                          lane={pane.lane_name ?? "-"} | mode={pane.lane_mode ?? "-"} | dead=
                          {String(pane.dead)}
                        </span>
                      </button>
                    );
                  })}
                </div>
              ) : (
                <p className="trace-note">No panes available.</p>
              )}
            </aside>
            <div className="trace-terminal-view">
              <h3>Live Pane Output</h3>
              {selectedPane ? (
                <p className="trace-note">
                  target={selectedPane.target} | pane_id={selectedPane.pane_id} | cmd=
                  {selectedPane.command || "-"}
                </p>
              ) : (
                <p className="trace-note">Select a pane to begin streaming output.</p>
              )}
              {selectedPane ? <p className="trace-note">Auto-refresh every 2s while selected.</p> : null}
              <div className="trace-terminal-controls">
                <label className="trace-field">
                  Command Input:
                  <input
                    value={paneInput}
                    disabled={!selectedPane || paneInputBusy}
                    placeholder="Type command text and press Enter to send"
                    onChange={(event) => setPaneInput(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        void sendPaneInput({
                          text: paneInput,
                          pressEnter: true,
                          clearTextAfterSend: true,
                        });
                        return;
                      }
                      if (event.ctrlKey && event.key.toLowerCase() === "c") {
                        event.preventDefault();
                        void sendPaneInput({ key: "C-c" });
                        return;
                      }
                      if (event.ctrlKey && event.key.toLowerCase() === "l") {
                        event.preventDefault();
                        void sendPaneInput({ key: "C-l" });
                      }
                    }}
                  />
                </label>
                <div className="trace-button-row">
                  <button
                    className="trace-btn"
                    onClick={() =>
                      void sendPaneInput({
                        text: paneInput,
                        pressEnter: true,
                        clearTextAfterSend: true,
                      })
                    }
                    disabled={!selectedPane || paneInputBusy}
                  >
                    Send Input
                  </button>
                  <button className="trace-btn" onClick={() => void sendPaneInput({ key: "Enter" })} disabled={!selectedPane || paneInputBusy}>
                    Enter
                  </button>
                  <button className="trace-btn" onClick={() => void sendPaneInput({ key: "C-c" })} disabled={!selectedPane || paneInputBusy}>
                    Ctrl+C
                  </button>
                  <button className="trace-btn" onClick={() => void sendPaneInput({ key: "Up" })} disabled={!selectedPane || paneInputBusy}>
                    Up
                  </button>
                  <button className="trace-btn" onClick={() => void sendPaneInput({ key: "Down" })} disabled={!selectedPane || paneInputBusy}>
                    Down
                  </button>
                  <button className="trace-btn" onClick={() => void sendPaneInput({ key: "Tab" })} disabled={!selectedPane || paneInputBusy}>
                    Tab
                  </button>
                </div>
              </div>
              {paneInputBusy ? <p className="trace-note">Sending pane input...</p> : null}
              {paneInputError ? <p className="trace-error">Pane input failed: {paneInputError}</p> : null}
              {paneInputLastAction ? <p className="trace-note">last_input={paneInputLastAction}</p> : null}
              {paneCaptureBusy ? <p className="trace-note">Refreshing pane output...</p> : null}
              {paneCaptureError ? <p className="trace-error">Pane capture failed: {paneCaptureError}</p> : null}
              {paneCapturedAt ? <p className="trace-note">captured_at={paneCapturedAt}</p> : null}
              <pre className="trace-console trace-terminal-console">
                {paneCaptureText || "No pane output captured yet."}
              </pre>
            </div>
          </div>
        </section>

        <section className="trace-panel">
          <h2>Agent Runs</h2>
          <p>Run multi-lane agent workflow and poll run status.</p>
          <fieldset className="trace-fieldset" disabled={agentBusy || Boolean(orchestrationBusy)}>
            <div className="trace-fields">
              <label className="trace-field">
                Session:
                <input value={session} onChange={(event) => setSession(event.target.value)} />
              </label>
              <label className="trace-field">
                Target:
                <input
                  value={agentTarget}
                  placeholder={defaultTmuxTarget}
                  onChange={(event) => setAgentTarget(event.target.value)}
                />
              </label>
              <label className="trace-field">
                Profiles (comma-separated):
                <input
                  value={agentProfiles}
                  onChange={(event) => setAgentProfiles(event.target.value)}
                />
              </label>
              <label className="trace-field">
                Runner Timeout (s):
                <input
                  value={agentRunnerTimeoutSec}
                  onChange={(event) => setAgentRunnerTimeoutSec(event.target.value)}
                />
              </label>
              <label className="trace-field">
                Report ID (optional):
                <input value={agentReportId} onChange={(event) => setAgentReportId(event.target.value)} />
              </label>
              <label className="trace-field">
                Output Mode:
                <select
                  value={agentOutputMode}
                  onChange={(event) => setAgentOutputMode(event.target.value as "codex" | "scripted")}
                >
                  <option value="codex">codex</option>
                  <option value="scripted">scripted</option>
                </select>
              </label>
              <label className="trace-field">
                Reasoning Effort:
                <select
                  value={agentReasoningEffort}
                  disabled={agentOutputMode !== "codex"}
                  onChange={(event) => setAgentReasoningEffort(event.target.value)}
                >
                  <option value="low">low</option>
                  <option value="medium">medium</option>
                  <option value="high">high</option>
                  <option value="xhigh">xhigh</option>
                </select>
              </label>
              <label className="trace-field">
                Task Count:
                <input value={agentTaskCount} onChange={(event) => setAgentTaskCount(event.target.value)} />
              </label>
              <label className="trace-field">
                Task Prefix:
                <input value={agentTaskPrefix} onChange={(event) => setAgentTaskPrefix(event.target.value)} />
              </label>
              <label className="trace-field">
                Input Source:
                <select
                  value={agentInputSource}
                  disabled={agentOutputMode !== "codex"}
                  onChange={(event) => setAgentInputSource(event.target.value as "predefined" | "human")}
                >
                  <option value="predefined">predefined</option>
                  <option value="human">human</option>
                </select>
              </label>
              {agentOutputMode === "codex" && agentInputSource === "human" ? (
                <label className="trace-field">
                  Human Prompt:
                  <textarea
                    value={agentHumanPrompt}
                    onChange={(event) => setAgentHumanPrompt(event.target.value)}
                    rows={4}
                    placeholder="Describe the task instruction you want agents to run."
                  />
                </label>
              ) : null}
            </div>
          </fieldset>
          <div className="trace-button-row">
            <button className="trace-btn trace-btn-primary" onClick={() => void runAgentWorkflow()} disabled={agentBusy || codexAuthBusy}>
              Run Agents
            </button>
            <button className="trace-btn" onClick={() => void refreshAgentRunStatus()} disabled={agentBusy || !agentRun?.run_id}>
              Refresh Status
            </button>
            <button className="trace-btn" onClick={() => void viewLatestReport()} disabled={reportBusy}>
              View Latest Report
            </button>
          </div>
          {agentBusy ? <p className="trace-note">Running agent action...</p> : null}
          {agentRun && !isAgentTerminal(agentRun.status) ? (
            <p className="trace-note">Auto-polling status while run is active.</p>
          ) : null}
          {agentError ? <p className="trace-error">Agent run failed: {agentError}</p> : null}
          {agentRun ? (
            <>
              <p className="trace-note">
                run_id={agentRun.run_id} | status={agentRun.status} | step={agentRun.current_step}
              </p>
              <p className="trace-note">
                output_mode={agentRun.runner_output_mode ?? "default"} | reasoning=
                {agentRun.runner_reasoning_effort ?? "default"} | task_count=
                {agentRun.runner_task_count ?? "default"} | task_prefix=
                {agentRun.runner_task_prefix ?? "default"}
              </p>
              {agentRun.error ? <p className="trace-error">run error: {agentRun.error}</p> : null}
              {agentRun.report_id ? <p className="trace-note">report_id={agentRun.report_id}</p> : null}
              {agentRun.summary ? (
                <pre className="trace-console">{JSON.stringify(agentRun.summary, null, 2)}</pre>
              ) : (
                <pre className="trace-console">No benchmark summary available yet.</pre>
              )}
            </>
          ) : (
            <pre className="trace-console">No agent run started yet.</pre>
          )}
        </section>

        <section className="trace-panel trace-panel-wide">
          <h2>Latest Report</h2>
          <p>Fetch the newest benchmark report and render model-level summary.</p>
          {reportBusy ? <p className="trace-note">Loading report...</p> : null}
          {reportError ? <p className="trace-error">Report retrieval failed: {reportError}</p> : null}
          {latestReport ? (
            <>
              <p className="trace-note">
                report_id={latestReport.report_id} | generated_at={latestReport.generated_at} | source=
                {latestReportSource ?? "n/a"}
              </p>
              <p className="trace-note">
                total_tasks={latestReport.total_tasks} | total_runs={latestReport.total_runs} | total_events=
                {latestReport.total_events}
              </p>
              <div className="trace-table-wrap">
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
              </div>
            </>
          ) : (
            <pre className="trace-console">No report loaded yet.</pre>
          )}
        </section>

        <section className="trace-panel">
          <h2>Tasks</h2>
          {tasksQuery.isLoading ? <p className="trace-note">Loading tasks...</p> : null}
          {tasksQuery.error ? <p className="trace-error">Task fetch failed: {(tasksQuery.error as Error).message}</p> : null}
          <ul className="trace-list">
            {(tasksQuery.data ?? []).map((task) => (
              <li key={task.task.task_id}>
                {task.task.task_id} | {task.status} | {task.task.title}
              </li>
            ))}
          </ul>
        </section>

        <section className="trace-panel">
          <h2>Candidates</h2>
          <label className="trace-field trace-checkbox">
            <input
              type="checkbox"
              checked={includeDisqualified}
              onChange={(event) => setIncludeDisqualified(event.target.checked)}
            />
            Show stale/disqualified
          </label>
          {candidatesQuery.isLoading ? <p className="trace-note">Loading candidates...</p> : null}
          {candidatesQuery.error ? (
            <p className="trace-error">Candidate fetch failed: {(candidatesQuery.error as Error).message}</p>
          ) : null}
          <ul className="trace-list">
            {visibleCandidates.map((candidate) => (
              <li key={candidate.candidate_id}>
                {candidate.candidate_id} | run={candidate.run_id} | eligible={String(candidate.eligible)}
                {candidate.disqualified_reason ? ` | reason=${candidate.disqualified_reason}` : ""}
              </li>
            ))}
          </ul>
        </section>

        <section className="trace-panel trace-panel-wide">
          <h2>Run Output</h2>
          {outputQuery.isLoading ? <p className="trace-note">Loading output...</p> : null}
          {outputQuery.error ? <p className="trace-error">Output fetch failed: {(outputQuery.error as Error).message}</p> : null}
          <pre className="trace-console">{outputText}</pre>
        </section>
      </main>
    </div>
  );
}
