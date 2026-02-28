import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { fetchCandidates, fetchRunOutput, fetchTasks } from "./api";
import { decodeOutputChunk, filterCandidates } from "./guards";

export default function App() {
  const [includeDisqualified, setIncludeDisqualified] = useState(false);

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

  return (
    <main style={{ fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", padding: 20 }}>
      <h1>TRACE Phase 0 Scaffold</h1>

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
