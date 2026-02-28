import { describe, expect, it } from "vitest";

import { decodeOutputChunk, parseTaskResponse } from "./guards";

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
