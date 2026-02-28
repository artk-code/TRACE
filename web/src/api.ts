import type {
  CandidateSummary,
  OutputChunk,
  TaskResponse,
  TimelineEvent,
  TmuxCommandResponse,
} from "./contracts";
import {
  parseCandidates,
  parseOutput,
  parseTaskList,
  parseTaskResponse,
  parseTimeline,
  parseTmuxCommandResponse,
} from "./guards";

const API_BASE = import.meta.env.VITE_TRACE_API_BASE_URL ?? "";

export type TmuxStartRequest = {
  session?: string;
  trace_root?: string;
  addr?: string;
};

export type TmuxSessionRequest = {
  session?: string;
};

export type TmuxAddLaneRequest = {
  session?: string;
  lane_name: string;
  profile?: string;
  mode?: string;
  wait_for_runner?: boolean;
  runner_timeout_sec?: number;
};

export type TmuxAddPaneRequest = {
  session?: string;
  lane_name: string;
  profile?: string;
  target?: string;
  mode?: string;
  wait_for_runner?: boolean;
  runner_timeout_sec?: number;
};

async function requestJson(path: string, init?: RequestInit): Promise<unknown> {
  const response = await fetch(`${API_BASE}${path}`, init);
  if (!response.ok) {
    const detail = await parseErrorDetail(response);
    const suffix = detail ? `: ${detail}` : "";
    throw new Error(`Request failed: ${response.status} ${response.statusText}${suffix}`);
  }

  return response.json();
}

async function getJson(path: string): Promise<unknown> {
  return requestJson(path);
}

async function postJson(path: string, body: unknown): Promise<unknown> {
  return requestJson(path, {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
}

async function parseErrorDetail(response: Response): Promise<string> {
  const contentType = response.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) {
    try {
      const payload = (await response.json()) as { error?: unknown };
      if (typeof payload.error === "string" && payload.error.trim() !== "") {
        return payload.error;
      }
    } catch {
      return "";
    }
  }

  try {
    const text = await response.text();
    return text.trim();
  } catch {
    return "";
  }
}

export async function fetchTasks(): Promise<TaskResponse[]> {
  const raw = await getJson("/tasks");
  return parseTaskList(raw);
}

export async function fetchTask(taskId: string): Promise<TaskResponse> {
  const raw = await getJson(`/tasks/${taskId}`);
  return parseTaskResponse(raw);
}

export async function fetchTaskTimeline(taskId: string): Promise<TimelineEvent[]> {
  const raw = await getJson(`/tasks/${taskId}/timeline`);
  return parseTimeline(raw);
}

export async function fetchRunTimeline(runId: string): Promise<TimelineEvent[]> {
  const raw = await getJson(`/runs/${runId}/timeline`);
  return parseTimeline(raw);
}

export async function fetchCandidates(
  taskId: string,
  includeDisqualified = false,
): Promise<CandidateSummary[]> {
  const query = includeDisqualified ? "true" : "false";
  const raw = await getJson(`/tasks/${taskId}/candidates?include_disqualified=${query}`);
  return parseCandidates(raw);
}

export async function fetchRunOutput(runId: string): Promise<OutputChunk[]> {
  const raw = await getJson(`/runs/${runId}/output`);
  return parseOutput(raw);
}

export async function postTmuxStart(request: TmuxStartRequest): Promise<TmuxCommandResponse> {
  const raw = await postJson("/orchestrator/tmux/start", request);
  return parseTmuxCommandResponse(raw);
}

export async function postTmuxStatus(request: TmuxSessionRequest): Promise<TmuxCommandResponse> {
  const raw = await postJson("/orchestrator/tmux/status", request);
  return parseTmuxCommandResponse(raw);
}

export async function postTmuxAddLane(request: TmuxAddLaneRequest): Promise<TmuxCommandResponse> {
  const raw = await postJson("/orchestrator/tmux/add-lane", request);
  return parseTmuxCommandResponse(raw);
}

export async function postTmuxAddPane(request: TmuxAddPaneRequest): Promise<TmuxCommandResponse> {
  const raw = await postJson("/orchestrator/tmux/add-pane", request);
  return parseTmuxCommandResponse(raw);
}

export async function postTmuxStop(request: TmuxSessionRequest): Promise<TmuxCommandResponse> {
  const raw = await postJson("/orchestrator/tmux/stop", request);
  return parseTmuxCommandResponse(raw);
}
