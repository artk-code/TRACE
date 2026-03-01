import { expect, test, type Route } from "@playwright/test";

function respondJson(route: Route, body: unknown, status = 200): Promise<void> {
  return route.fulfill({
    status,
    contentType: "application/json",
    headers: {
      "access-control-allow-origin": "*",
    },
    body: JSON.stringify(body),
  });
}

test("tmux workspace: load session tree and stream selected pane output", async ({ page }) => {
  let captureCalls = 0;
  let snapshotCalls = 0;
  let sendKeysCalls = 0;
  let lastSendPayload: Record<string, unknown> | null = null;

  await page.route("**/tasks", async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    return respondJson(route, []);
  });

  await page.route("**/orchestrator/tmux/snapshot", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    snapshotCalls += 1;
    return respondJson(route, {
      session: "trace-smoke-ui",
      windows: [
        {
          window_index: 0,
          window_name: "lanes",
          window_id: "@1",
          active: true,
        },
      ],
      panes: [
        {
          pane_id: "%1",
          session: "trace-smoke-ui",
          window_index: 0,
          window_name: "lanes",
          pane_index: 0,
          target: "trace-smoke-ui:lanes.0",
          title: "lane-flash",
          lane_name: "flash",
          lane_mode: "runner",
          active: true,
          dead: false,
          dead_status: 0,
          pid: 12345,
          command: "bash",
        },
        {
          pane_id: "%2",
          session: "trace-smoke-ui",
          window_index: 0,
          window_name: "lanes",
          pane_index: 1,
          target: "trace-smoke-ui:lanes.1",
          title: "lane-high",
          lane_name: "high",
          lane_mode: "runner",
          active: false,
          dead: false,
          dead_status: 0,
          pid: 12346,
          command: "bash",
        },
      ],
      config: {
        trace_root: "/tmp/trace-smoke-ui",
        trace_server_addr: "127.0.0.1:18086",
        runner_output_mode: "codex",
        runner_task_count: "1",
        runner_task_prefix: "TASK-SMOKE",
        runner_reasoning_effort: "low",
      },
    });
  });

  await page.route("**/orchestrator/tmux/capture", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    captureCalls += 1;
    return respondJson(route, {
      session: "trace-smoke-ui",
      target: "%1",
      lines: 200,
      captured_at: `2026-03-01T09:00:0${Math.min(captureCalls, 9)}Z`,
      content: `pane sample line ${captureCalls}\n`,
    });
  });

  await page.route("**/orchestrator/tmux/send-keys", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    sendKeysCalls += 1;
    const postBody = route.request().postData() ?? "{}";
    lastSendPayload = JSON.parse(postBody) as Record<string, unknown>;
    return respondJson(route, {
      command: "scripts/trace-smoke-tmux.sh --session trace-smoke-ui send-keys",
      exit_code: 0,
      stdout: "sent keys",
      stderr: "",
    });
  });

  await page.goto("/");

  await page.getByRole("button", { name: "Load Session Tree" }).click();
  await expect(page.getByText("session=trace-smoke-ui | windows=1 | panes=2")).toBeVisible();
  await expect(page.getByRole("button", { name: /lanes\.0 \(%1\)/ })).toBeVisible();
  await expect(page.getByText("Auto-refresh every 2s while selected.")).toBeVisible();
  await expect(page.getByText(/pane sample line/)).toBeVisible();
  expect(snapshotCalls).toBe(1);

  await page.getByLabel("Command Input:").fill("echo live-from-ui");
  await page.getByRole("button", { name: "Send Input" }).click();
  await expect(page.getByText('last_input=text="echo live-from-ui" | enter=true')).toBeVisible();
  expect(sendKeysCalls).toBeGreaterThan(0);
  expect(lastSendPayload?.target).toBe("%1");
  expect(lastSendPayload?.text).toBe("echo live-from-ui");
  expect(lastSendPayload?.press_enter).toBe(true);

  await page.getByRole("button", { name: "Ctrl+C" }).click();
  await expect(page.getByText("last_input=key=C-c")).toBeVisible();

  await page.getByRole("button", { name: "Reconnect Stream" }).click();
  expect(snapshotCalls).toBeGreaterThanOrEqual(2);
});
