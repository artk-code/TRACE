import type { CandidateSummary, OutputChunk, TaskResponse, TimelineEvent } from "./contracts";
import { parseCandidates, parseOutput, parseTaskList, parseTaskResponse, parseTimeline } from "./guards";

const API_BASE = import.meta.env.VITE_TRACE_API_BASE_URL ?? "";

async function getJson(path: string): Promise<unknown> {
  const response = await fetch(`${API_BASE}${path}`);
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status} ${response.statusText}`);
  }

  return response.json();
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
