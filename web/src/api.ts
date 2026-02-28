import type { CandidateSummary, OutputChunk, TaskResponse, TimelineEvent } from "./contracts";
import { parseTaskList, parseTaskResponse } from "./guards";

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status} ${response.statusText}`);
  }

  return (await response.json()) as T;
}

export async function fetchTasks(): Promise<TaskResponse[]> {
  const raw = await getJson<unknown>("/tasks");
  return parseTaskList(raw);
}

export async function fetchTask(taskId: string): Promise<TaskResponse> {
  const raw = await getJson<unknown>(`/tasks/${taskId}`);
  return parseTaskResponse(raw);
}

export async function fetchTaskTimeline(taskId: string): Promise<TimelineEvent[]> {
  return getJson<TimelineEvent[]>(`/tasks/${taskId}/timeline`);
}

export async function fetchRunTimeline(runId: string): Promise<TimelineEvent[]> {
  return getJson<TimelineEvent[]>(`/runs/${runId}/timeline`);
}

export async function fetchCandidates(
  taskId: string,
  includeDisqualified = false,
): Promise<CandidateSummary[]> {
  const query = includeDisqualified ? "true" : "false";
  return getJson<CandidateSummary[]>(
    `/tasks/${taskId}/candidates?include_disqualified=${query}`,
  );
}

export async function fetchRunOutput(runId: string): Promise<OutputChunk[]> {
  return getJson<OutputChunk[]>(`/runs/${runId}/output`);
}
