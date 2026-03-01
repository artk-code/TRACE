import { z } from "zod";

export const taskStatusSchema = z.enum([
  "Unclaimed",
  "Claimed",
  "Running",
  "Evaluating",
  "Reviewed",
  "Done",
]);

export const taskSchema = z
  .object({
    task_id: z.string(),
    title: z.string(),
    owner: z.string().optional(),
  })
  .strict();

export const statusDetailSchema = z
  .object({
    lease_epoch: z.number().int().nonnegative().optional(),
    holder: z.string().optional(),
    reason: z.string().optional(),
  })
  .strict();

export const taskResponseSchema = z
  .object({
    task: taskSchema,
    status: taskStatusSchema,
    status_detail: statusDetailSchema.optional(),
  })
  .strict();

export const timelineEventSchema = z
  .object({
    kind: z.string(),
    ts: z.string(),
    task_id: z.string(),
    run_id: z.string().optional().nullable(),
  })
  .strict();

export const candidateSummarySchema = z
  .object({
    candidate_id: z.string(),
    task_id: z.string(),
    run_id: z.string(),
    lease_epoch: z.number().int().nonnegative(),
    eligible: z.boolean(),
    disqualified_reason: z.string().optional().nullable(),
  })
  .strict();

export const outputChunkSchema = z
  .object({
    stream: z.enum(["stdout", "stderr"]),
    encoding: z.enum(["utf8", "base64"]),
    chunk: z.string(),
    chunk_index: z.number().int().nonnegative(),
    final: z.boolean().optional(),
  })
  .strict();

export const tmuxCommandResponseSchema = z
  .object({
    command: z.string(),
    exit_code: z.number().int(),
    stdout: z.string(),
    stderr: z.string(),
  })
  .strict();

export const codexAuthStatusSchema = z
  .object({
    command: z.string(),
    policy: z.enum(["required", "optional"]),
    available: z.boolean(),
    logged_in: z.boolean(),
    method: z.string().optional().nullable(),
    requires_login: z.boolean(),
    exit_code: z.number().int(),
    stdout: z.string(),
    stderr: z.string(),
    login_commands: z.array(z.string()),
  })
  .strict();

export const benchmarkModelSummarySchema = z
  .object({
    model_key: z.string(),
    model: z.string().optional().nullable(),
    provider: z.string().optional().nullable(),
    profile: z.string().optional().nullable(),
    runs: z.number().int().nonnegative(),
    pass_count: z.number().int().nonnegative(),
    fail_count: z.number().int().nonnegative(),
    candidate_total: z.number().int().nonnegative(),
    candidate_eligible: z.number().int().nonnegative(),
    candidate_disqualified: z.number().int().nonnegative(),
    output_bytes: z.number().int().nonnegative(),
    avg_duration_ms: z.number().optional().nullable(),
  })
  .strict();

export const benchmarkSummarySchema = z
  .object({
    total_tasks: z.number().int().nonnegative(),
    total_runs: z.number().int().nonnegative(),
    total_events: z.number().int().nonnegative(),
    models: z.array(benchmarkModelSummarySchema),
  })
  .strict();

export const benchmarkRunSummarySchema = z
  .object({
    run_id: z.string(),
    task_id: z.string(),
    model: z.string().optional().nullable(),
    provider: z.string().optional().nullable(),
    profile: z.string().optional().nullable(),
    worker_id: z.string().optional().nullable(),
    lease_epoch: z.number().int().nonnegative().optional().nullable(),
    started_at: z.string().optional().nullable(),
    completed_at: z.string().optional().nullable(),
    duration_ms: z.number().int().optional().nullable(),
    candidate_total: z.number().int().nonnegative(),
    candidate_eligible: z.number().int().nonnegative(),
    candidate_disqualified: z.number().int().nonnegative(),
    output_chunks: z.number().int().nonnegative(),
    output_bytes: z.number().int().nonnegative(),
    verdict: z.string().optional().nullable(),
    passed: z.boolean().optional().nullable(),
  })
  .strict();

export const benchmarkReportSchema = z
  .object({
    report_id: z.string(),
    generated_at: z.string(),
    total_events: z.number().int().nonnegative(),
    total_tasks: z.number().int().nonnegative(),
    total_runs: z.number().int().nonnegative(),
    models: z.array(benchmarkModelSummarySchema),
    runs: z.array(benchmarkRunSummarySchema),
  })
  .strict();

export const reportListItemSchema = z
  .object({
    report_id: z.string(),
    generated_at: z.string(),
    total_events: z.number().int().nonnegative(),
    total_tasks: z.number().int().nonnegative(),
    total_runs: z.number().int().nonnegative(),
    models: z.array(benchmarkModelSummarySchema),
  })
  .strict();

export const reportListResponseSchema = z
  .object({
    reports: z.array(reportListItemSchema),
  })
  .strict();

export const agentRunStatusSchema = z.enum(["queued", "running", "succeeded", "failed"]);

export const agentRunResponseSchema = z
  .object({
    run_id: z.string(),
    status: agentRunStatusSchema,
    created_at: z.string(),
    updated_at: z.string(),
    session: z.string(),
    target: z.string(),
    profiles: z.array(z.string()),
    lane_names: z.array(z.string()),
    runner_timeout_sec: z.number().int().positive(),
    runner_output_mode: z.enum(["codex", "scripted"]).optional().nullable(),
    runner_task_count: z.number().int().positive().optional().nullable(),
    runner_task_prefix: z.string().optional().nullable(),
    runner_reasoning_effort: z.string().optional().nullable(),
    runner_codex_prompt: z.string().optional().nullable(),
    current_step: z.string(),
    error: z.string().optional().nullable(),
    report_id: z.string().optional().nullable(),
    json_report_path: z.string().optional().nullable(),
    markdown_report_path: z.string().optional().nullable(),
    summary: benchmarkSummarySchema.optional().nullable(),
  })
  .strict();

export type TaskStatus = z.infer<typeof taskStatusSchema>;
export type TaskResponse = z.infer<typeof taskResponseSchema>;
export type TimelineEvent = z.infer<typeof timelineEventSchema>;
export type CandidateSummary = z.infer<typeof candidateSummarySchema>;
export type OutputChunk = z.infer<typeof outputChunkSchema>;
export type TmuxCommandResponse = z.infer<typeof tmuxCommandResponseSchema>;
export type CodexAuthStatus = z.infer<typeof codexAuthStatusSchema>;
export type BenchmarkModelSummary = z.infer<typeof benchmarkModelSummarySchema>;
export type BenchmarkSummary = z.infer<typeof benchmarkSummarySchema>;
export type BenchmarkRunSummary = z.infer<typeof benchmarkRunSummarySchema>;
export type BenchmarkReport = z.infer<typeof benchmarkReportSchema>;
export type ReportListItem = z.infer<typeof reportListItemSchema>;
export type ReportListResponse = z.infer<typeof reportListResponseSchema>;
export type AgentRunStatus = z.infer<typeof agentRunStatusSchema>;
export type AgentRunResponse = z.infer<typeof agentRunResponseSchema>;
