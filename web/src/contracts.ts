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

export type TaskStatus = z.infer<typeof taskStatusSchema>;
export type TaskResponse = z.infer<typeof taskResponseSchema>;
export type TimelineEvent = z.infer<typeof timelineEventSchema>;
export type CandidateSummary = z.infer<typeof candidateSummarySchema>;
export type OutputChunk = z.infer<typeof outputChunkSchema>;
