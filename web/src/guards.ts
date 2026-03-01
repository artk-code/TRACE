import {
  benchmarkReportSchema,
  candidateSummarySchema,
  codexAuthStatusSchema,
  outputChunkSchema,
  reportListResponseSchema,
  smokeRunResponseSchema,
  taskResponseSchema,
  tmuxCommandResponseSchema,
  timelineEventSchema,
  type BenchmarkReport,
  type CandidateSummary,
  type CodexAuthStatus,
  type OutputChunk,
  type ReportListResponse,
  type SmokeRunResponse,
  type TaskResponse,
  type TmuxCommandResponse,
  type TimelineEvent,
} from "./contracts";

declare const Buffer:
  | {
      from: (value: string, encoding: string) => { toString: (encoding: string) => string; length: number };
    }
  | undefined;

const taskListSchema = taskResponseSchema.array();
const timelineListSchema = timelineEventSchema.array();
const candidateListSchema = candidateSummarySchema.array();
const outputListSchema = outputChunkSchema.array();
const tmuxCommandSchema = tmuxCommandResponseSchema;
const codexAuthStatusGuard = codexAuthStatusSchema;
const smokeRunResponseGuard = smokeRunResponseSchema;
const reportListResponseGuard = reportListResponseSchema;
const benchmarkReportGuard = benchmarkReportSchema;

export function parseTaskResponse(raw: unknown): TaskResponse {
  return taskResponseSchema.parse(raw);
}

export function parseTaskList(raw: unknown): TaskResponse[] {
  return taskListSchema.parse(raw);
}

export function parseTimeline(raw: unknown): TimelineEvent[] {
  return timelineListSchema.parse(raw);
}

export function parseCandidates(raw: unknown): CandidateSummary[] {
  return candidateListSchema.parse(raw);
}

export function parseOutput(raw: unknown): OutputChunk[] {
  return outputListSchema.parse(raw);
}

export function parseTmuxCommandResponse(raw: unknown): TmuxCommandResponse {
  return tmuxCommandSchema.parse(raw);
}

export function parseCodexAuthStatus(raw: unknown): CodexAuthStatus {
  return codexAuthStatusGuard.parse(raw);
}

export function parseSmokeRunResponse(raw: unknown): SmokeRunResponse {
  return smokeRunResponseGuard.parse(raw);
}

export function parseReportListResponse(raw: unknown): ReportListResponse {
  return reportListResponseGuard.parse(raw);
}

export function parseBenchmarkReport(raw: unknown): BenchmarkReport {
  return benchmarkReportGuard.parse(raw);
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
    const byteLength = utf8ByteLength(chunk.chunk);
    if (byteLength > maxBytes) {
      throw new Error(`utf8 chunk exceeded limit (${maxBytes} bytes)`);
    }
    return chunk.chunk;
  }

  const decoded = base64ToText(chunk.chunk);
  if (decoded.byteLength > maxBytes) {
    throw new Error(`base64 decoded payload exceeded limit (${maxBytes} bytes)`);
  }

  return decoded.text;
}

function base64ToText(value: string): { text: string; byteLength: number } {
  if (typeof Buffer !== "undefined") {
    const bytes = Buffer.from(value, "base64");
    return { text: bytes.toString("utf8"), byteLength: bytes.length };
  }

  if (typeof atob === "function") {
    const binary = atob(value);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }

    const text = new TextDecoder().decode(bytes);
    return { text, byteLength: bytes.length };
  }

  throw new Error("No base64 decoder available in this runtime");
}

function utf8ByteLength(value: string): number {
  if (typeof TextEncoder !== "undefined") {
    return new TextEncoder().encode(value).length;
  }

  return value.length;
}
