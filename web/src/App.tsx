import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import type { TmuxCommandResponse } from "./contracts";
import {
  fetchCandidates,
  fetchRunOutput,
  fetchTasks,
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

export default function App() {
  const [includeDisqualified, setIncludeDisqualified] = useState(false);
  const [session, setSession] = useState("trace-smoke");
  const [traceRoot, setTraceRoot] = useState("/tmp/trace-web-smoke");
  const [serverAddr, setServerAddr] = useState("127.0.0.1:18086");
  const [laneName, setLaneName] = useState("codex4");
  const [laneProfile, setLaneProfile] = useState("high");
  const [paneLaneName, setPaneLaneName] = useState("codex5");
  const [paneProfile, setPaneProfile] = useState("flash");
  const [paneTarget, setPaneTarget] = useState("");
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

  const defaultPaneTarget = useMemo(() => {
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

  return (
    <main style={{ fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", padding: 20 }}>
      <h1>TRACE Phase 0 Scaffold</h1>

      <section>
        <h2>Orchestration</h2>
        <p>Control tmux session lifecycle through TRACE API orchestration routes.</p>
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
            Add-Pane Name:
            <input value={paneLaneName} onChange={(event) => setPaneLaneName(event.target.value)} />
          </label>
          <label>
            Add-Pane Profile:
            <input value={paneProfile} onChange={(event) => setPaneProfile(event.target.value)} />
          </label>
          <label>
            Add-Pane Target:
            <input
              value={paneTarget}
              placeholder={defaultPaneTarget}
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
              runOrchestrationAction("add-lane", () =>
                postTmuxAddLane({
                  session: optionalValue(session),
                  lane_name: laneName.trim(),
                  profile: optionalValue(laneProfile),
                }),
              )
            }
            disabled={Boolean(orchestrationBusy)}
          >
            Add Lane
          </button>{" "}
          <button
            onClick={() =>
              runOrchestrationAction("add-pane", () =>
                postTmuxAddPane({
                  session: optionalValue(session),
                  lane_name: paneLaneName.trim(),
                  profile: optionalValue(paneProfile),
                  target: optionalValue(paneTarget) ?? defaultPaneTarget,
                }),
              )
            }
            disabled={Boolean(orchestrationBusy)}
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
