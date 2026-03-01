import { describe, expect, it } from "vitest";

import {
  decodeOutputChunk,
  parseBenchmarkReport,
  parseCodexAuthStatus,
  parseReportListResponse,
  parseSmokeRunResponse,
  parseTaskResponse,
  parseTmuxCommandResponse,
} from "./guards";

describe("task_response_guard_accepts_nested_shape", () => {
  it("accepts nested TaskResponse shape", () => {
    const payload = {
      task: {
        task_id: "TASK-42",
        title: "Improve lease replay",
      },
      status: "Claimed",
    };

    expect(parseTaskResponse(payload).task.task_id).toBe("TASK-42");
  });
});

describe("task_response_guard_rejects_flat_shape", () => {
  it("rejects flat TaskResponse shape", () => {
    const payload = {
      task_id: "TASK-42",
      title: "Improve lease replay",
      status: "Claimed",
    };

    expect(() => parseTaskResponse(payload)).toThrow();
  });
});

describe("output_decoder_handles_utf8_and_base64_with_limits", () => {
  it("decodes utf8 and base64", () => {
    expect(
      decodeOutputChunk({
        stream: "stdout",
        encoding: "utf8",
        chunk: "hello",
        chunk_index: 0,
      }),
    ).toBe("hello");

    expect(
      decodeOutputChunk({
        stream: "stdout",
        encoding: "base64",
        chunk: "aGVsbG8=",
        chunk_index: 1,
      }),
    ).toBe("hello");
  });

  it("enforces size limits", () => {
    expect(() =>
      decodeOutputChunk(
        {
          stream: "stdout",
          encoding: "utf8",
          chunk: "hello",
          chunk_index: 0,
        },
        3,
      ),
    ).toThrow();
  });
});

describe("tmux_command_response_guard", () => {
  it("accepts orchestration command response shape", () => {
    const payload = {
      command: "scripts/trace-smoke-tmux.sh --session trace-smoke status",
      exit_code: 0,
      stdout: "windows:\n...",
      stderr: "",
    };

    expect(parseTmuxCommandResponse(payload).exit_code).toBe(0);
  });

  it("rejects invalid orchestration command response shape", () => {
    const payload = {
      command: "scripts/trace-smoke-tmux.sh --session trace-smoke status",
      exit_code: "0",
      stdout: "windows:\n...",
      stderr: "",
    };

    expect(() => parseTmuxCommandResponse(payload)).toThrow();
  });
});

describe("codex_auth_status_guard", () => {
  it("accepts codex auth status response shape", () => {
    const payload = {
      command: "codex login status",
      policy: "required",
      available: true,
      logged_in: true,
      method: "chatgpt",
      requires_login: false,
      exit_code: 0,
      stdout: "Logged in using ChatGPT",
      stderr: "",
      login_commands: ["codex login"],
    };

    expect(parseCodexAuthStatus(payload).logged_in).toBe(true);
  });

  it("rejects invalid codex auth status response shape", () => {
    const payload = {
      command: "codex login status",
      policy: "required",
      available: "yes",
      logged_in: true,
      method: "chatgpt",
      requires_login: false,
      exit_code: 0,
      stdout: "Logged in using ChatGPT",
      stderr: "",
      login_commands: ["codex login"],
    };

    expect(() => parseCodexAuthStatus(payload)).toThrow();
  });
});

describe("smoke_run_response_guard", () => {
  it("accepts smoke run response shape", () => {
    const payload = {
      run_id: "smoke-123",
      status: "running",
      created_at: "2026-03-01T08:00:00Z",
      updated_at: "2026-03-01T08:00:01Z",
      session: "trace-smoke",
      target: "trace-smoke:lanes",
      profiles: ["flash", "high", "extra"],
      lane_names: ["smoke-flash-123", "smoke-high-123", "smoke-extra-123"],
      runner_timeout_sec: 180,
      current_step: "waiting_for_lanes",
      error: null,
      report_id: null,
      json_report_path: null,
      markdown_report_path: null,
      summary: null,
    };

    expect(parseSmokeRunResponse(payload).status).toBe("running");
  });

  it("rejects invalid smoke run status", () => {
    const payload = {
      run_id: "smoke-123",
      status: "in_progress",
      created_at: "2026-03-01T08:00:00Z",
      updated_at: "2026-03-01T08:00:01Z",
      session: "trace-smoke",
      target: "trace-smoke:lanes",
      profiles: ["flash"],
      lane_names: ["smoke-flash-123"],
      runner_timeout_sec: 180,
      current_step: "queued",
      error: null,
      report_id: null,
      json_report_path: null,
      markdown_report_path: null,
      summary: null,
    };

    expect(() => parseSmokeRunResponse(payload)).toThrow();
  });
});

describe("report_list_response_guard", () => {
  it("accepts empty reports list response", () => {
    const payload = {
      reports: [],
    };

    expect(parseReportListResponse(payload).reports).toEqual([]);
  });

  it("accepts reports list response shape", () => {
    const payload = {
      reports: [
        {
          report_id: "report-123",
          generated_at: "2026-03-01T08:00:00Z",
          total_events: 42,
          total_tasks: 3,
          total_runs: 9,
          models: [
            {
              model_key: "openai:gpt-5:high",
              model: "gpt-5",
              provider: "openai",
              profile: "high",
              runs: 9,
              pass_count: 7,
              fail_count: 2,
              candidate_total: 9,
              candidate_eligible: 8,
              candidate_disqualified: 1,
              output_bytes: 2048,
              avg_duration_ms: 3500,
            },
          ],
        },
      ],
    };

    expect(parseReportListResponse(payload).reports[0]?.report_id).toBe("report-123");
  });

  it("rejects invalid reports list response shape", () => {
    const payload = {
      reports: {},
    };

    expect(() => parseReportListResponse(payload)).toThrow();
  });
});

describe("benchmark_report_guard", () => {
  it("accepts benchmark report with nullable optional run fields", () => {
    const payload = {
      report_id: "report-optional",
      generated_at: "2026-03-01T08:00:00Z",
      total_events: 1,
      total_tasks: 1,
      total_runs: 1,
      models: [
        {
          model_key: "unknown-provider:unknown-model:unknown-profile",
          model: null,
          provider: null,
          profile: null,
          runs: 1,
          pass_count: 0,
          fail_count: 0,
          candidate_total: 0,
          candidate_eligible: 0,
          candidate_disqualified: 0,
          output_bytes: 0,
          avg_duration_ms: null,
        },
      ],
      runs: [
        {
          run_id: "RUN-OPTIONAL-1",
          task_id: "TASK-OPTIONAL-1",
          model: null,
          provider: null,
          profile: null,
          worker_id: null,
          lease_epoch: null,
          started_at: null,
          completed_at: null,
          duration_ms: null,
          candidate_total: 0,
          candidate_eligible: 0,
          candidate_disqualified: 0,
          output_chunks: 0,
          output_bytes: 0,
          verdict: null,
          passed: null,
        },
      ],
    };

    expect(parseBenchmarkReport(payload).runs[0]?.run_id).toBe("RUN-OPTIONAL-1");
  });

  it("accepts benchmark report response shape", () => {
    const payload = {
      report_id: "report-123",
      generated_at: "2026-03-01T08:00:00Z",
      total_events: 42,
      total_tasks: 3,
      total_runs: 9,
      models: [
        {
          model_key: "openai:gpt-5:high",
          model: "gpt-5",
          provider: "openai",
          profile: "high",
          runs: 9,
          pass_count: 7,
          fail_count: 2,
          candidate_total: 9,
          candidate_eligible: 8,
          candidate_disqualified: 1,
          output_bytes: 2048,
          avg_duration_ms: 3500,
        },
      ],
      runs: [
        {
          run_id: "RUN-1",
          task_id: "TASK-1",
          model: "gpt-5",
          provider: "openai",
          profile: "high",
          worker_id: "worker-1",
          lease_epoch: 2,
          started_at: "2026-03-01T07:59:00Z",
          completed_at: "2026-03-01T08:00:00Z",
          duration_ms: 60000,
          candidate_total: 1,
          candidate_eligible: 1,
          candidate_disqualified: 0,
          output_chunks: 4,
          output_bytes: 512,
          verdict: "pass",
          passed: true,
        },
      ],
    };

    expect(parseBenchmarkReport(payload).runs[0]?.run_id).toBe("RUN-1");
  });

  it("rejects invalid benchmark report response shape", () => {
    const payload = {
      report_id: "report-123",
      generated_at: "2026-03-01T08:00:00Z",
      total_events: 42,
      total_tasks: 3,
      total_runs: "9",
      models: [],
      runs: [],
    };

    expect(() => parseBenchmarkReport(payload)).toThrow();
  });
});
