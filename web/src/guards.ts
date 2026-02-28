import type { CandidateSummary, OutputChunk, TaskResponse } from "./contracts";

declare const Buffer:
  | {
      from: (value: string, encoding: string) => { toString: (encoding: string) => string };
    }
  | undefined;

export function parseTaskResponse(raw: unknown): TaskResponse {
  if (!isRecord(raw)) {
    throw new Error("TaskResponse must be an object");
  }

  if (!isRecord(raw.task)) {
    throw new Error("TaskResponse.task must be an object");
  }

  if (typeof raw.task.task_id !== "string") {
    throw new Error("TaskResponse.task.task_id must be a string");
  }

  if (typeof raw.task.title !== "string") {
    throw new Error("TaskResponse.task.title must be a string");
  }

  if (typeof raw.status !== "string") {
    throw new Error("TaskResponse.status must be a string");
  }

  return raw as TaskResponse;
}

export function parseTaskList(raw: unknown): TaskResponse[] {
  if (!Array.isArray(raw)) {
    throw new Error("Task list response must be an array");
  }

  return raw.map(parseTaskResponse);
}

export function filterCandidates(
  candidates: CandidateSummary[],
  includeDisqualified: boolean,
): CandidateSummary[] {
  if (includeDisqualified) {
    return candidates;
  }

  return candidates.filter((candidate) => candidate.eligible);
}

export function decodeOutputChunk(chunk: OutputChunk, maxBytes = 64 * 1024): string {
  if (chunk.encoding === "utf8") {
    if (chunk.chunk.length > maxBytes) {
      throw new Error(`utf8 chunk exceeded limit (${maxBytes} bytes)`);
    }
    return chunk.chunk;
  }

  const decoded = base64ToText(chunk.chunk);
  if (decoded.length > maxBytes) {
    throw new Error(`base64 decoded payload exceeded limit (${maxBytes} bytes)`);
  }
  return decoded;
}

function base64ToText(value: string): string {
  if (typeof atob === "function") {
    return atob(value);
  }

  if (typeof Buffer !== "undefined") {
    return Buffer.from(value, "base64").toString("utf8");
  }

  throw new Error("No base64 decoder available in this runtime");
}

function isRecord(value: unknown): value is Record<string, any> {
  return typeof value === "object" && value !== null;
}
