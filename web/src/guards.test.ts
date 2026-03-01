import { describe, expect, it } from "vitest";

import {
  decodeOutputChunk,
  parseCodexAuthStatus,
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
