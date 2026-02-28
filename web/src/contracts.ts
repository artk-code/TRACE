export type TaskStatus =
  | "Unclaimed"
  | "Claimed"
  | "Running"
  | "Evaluating"
  | "Reviewed"
  | "Done";

export type TaskResponse = {
  task: {
    task_id: string;
    title: string;
    owner?: string;
  };
  status: TaskStatus;
  status_detail?: {
    lease_epoch?: number;
    holder?: string;
    reason?: string;
  };
};

export type TimelineEvent = {
  kind: string;
  ts: string;
  task_id: string;
  run_id?: string;
};

export type CandidateSummary = {
  candidate_id: string;
  task_id: string;
  run_id: string;
  lease_epoch: number;
  eligible: boolean;
  disqualified_reason?: string;
};

export type OutputChunk = {
  stream: "stdout" | "stderr";
  encoding: "utf8" | "base64";
  chunk: string;
  chunk_index: number;
  final?: boolean;
};
