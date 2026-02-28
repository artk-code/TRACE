import { useMemo, useState } from "react";

import type { CandidateSummary, OutputChunk, TaskResponse } from "./contracts";
import { decodeOutputChunk, filterCandidates } from "./guards";

const initialTasks: TaskResponse[] = [
  {
    task: { task_id: "TASK-42", title: "Improve lease replay", owner: "platform" },
    status: "Claimed",
    status_detail: { lease_epoch: 7, holder: "agent-3" },
  },
];

const initialCandidates: CandidateSummary[] = [
  {
    candidate_id: "C-100",
    task_id: "TASK-42",
    run_id: "RUN-13",
    lease_epoch: 7,
    eligible: true,
  },
  {
    candidate_id: "C-099",
    task_id: "TASK-42",
    run_id: "RUN-12",
    lease_epoch: 6,
    eligible: false,
    disqualified_reason: "stale_epoch",
  },
];

const initialOutput: OutputChunk[] = [
  {
    stream: "stdout",
    encoding: "utf8",
    chunk: "hello from RUN-13",
    chunk_index: 0,
    final: true,
  },
];

export default function App() {
  const [includeDisqualified, setIncludeDisqualified] = useState(false);

  const visibleCandidates = useMemo(
    () => filterCandidates(initialCandidates, includeDisqualified),
    [includeDisqualified],
  );

  const outputText = useMemo(
    () => initialOutput.map((chunk) => decodeOutputChunk(chunk)).join("\n"),
    [],
  );

  return (
    <main style={{ fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", padding: 20 }}>
      <h1>TRACE Phase 0 Scaffold</h1>

      <section>
        <h2>Tasks</h2>
        <ul>
          {initialTasks.map((task) => (
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
        <pre>{outputText}</pre>
      </section>
    </main>
  );
}
