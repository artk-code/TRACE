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

test("phase0 agent flow: auth check to agent run to report view", async ({ page }) => {
  const agentRunId = "agent-e2e-run";
  let agentStatusPolls = 0;

  const authStatus = {
    command: "codex login status",
    policy: "required",
    available: true,
    logged_in: true,
    method: "chatgpt",
    requires_login: false,
    exit_code: 0,
    stdout: "Logged in using ChatGPT",
    stderr: "",
    login_commands: ["codex login", "codex login --device-auth"],
  };

  await page.route("**/tasks", async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    return respondJson(route, []);
  });

  await page.route("**/orchestrator/auth/codex/status", async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    return respondJson(route, authStatus);
  });

  await page.route("**/agent/runs", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    return respondJson(route, {
      run_id: agentRunId,
      status: "queued",
      created_at: "2026-03-01T09:00:00Z",
      updated_at: "2026-03-01T09:00:00Z",
      session: "trace-smoke",
      target: "trace-smoke:lanes",
      profiles: ["flash", "high", "extra"],
      lane_names: ["agent-flash", "agent-high", "agent-extra"],
      runner_timeout_sec: 180,
      current_step: "queued",
      error: null,
      report_id: null,
      json_report_path: null,
      markdown_report_path: null,
      summary: null,
    });
  });

  await page.route(`**/agent/runs/${agentRunId}`, async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    agentStatusPolls += 1;
    if (agentStatusPolls === 1) {
      return respondJson(route, {
        run_id: agentRunId,
        status: "running",
        created_at: "2026-03-01T09:00:00Z",
        updated_at: "2026-03-01T09:00:01Z",
        session: "trace-smoke",
        target: "trace-smoke:lanes",
        profiles: ["flash", "high", "extra"],
        lane_names: ["agent-flash", "agent-high", "agent-extra"],
        runner_timeout_sec: 180,
        current_step: "waiting_for_lanes",
        error: null,
        report_id: null,
        json_report_path: null,
        markdown_report_path: null,
        summary: null,
      });
    }
    return respondJson(route, {
      run_id: agentRunId,
      status: "succeeded",
      created_at: "2026-03-01T09:00:00Z",
      updated_at: "2026-03-01T09:00:03Z",
      session: "trace-smoke",
      target: "trace-smoke:lanes",
      profiles: ["flash", "high", "extra"],
      lane_names: ["agent-flash", "agent-high", "agent-extra"],
      runner_timeout_sec: 180,
      current_step: "completed",
      error: null,
      report_id: "report-e2e",
      json_report_path: ".trace/reports/report-e2e.json",
      markdown_report_path: ".trace/reports/report-e2e.md",
      summary: {
        total_tasks: 3,
        total_runs: 9,
        total_events: 54,
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
    });
  });

  await page.route("**/reports?limit=1", async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    return respondJson(route, {
      reports: [
        {
          report_id: "report-e2e",
          generated_at: "2026-03-01T09:00:03Z",
          total_events: 54,
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
    });
  });

  await page.route("**/reports/report-e2e", async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    return respondJson(route, {
      report_id: "report-e2e",
      generated_at: "2026-03-01T09:00:03Z",
      total_events: 54,
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
          run_id: "RUN-E2E-1",
          task_id: "TASK-E2E-1",
          model: "gpt-5",
          provider: "openai",
          profile: "high",
          worker_id: "worker-e2e-1",
          lease_epoch: 1,
          started_at: "2026-03-01T09:00:00Z",
          completed_at: "2026-03-01T09:00:02Z",
          duration_ms: 2000,
          candidate_total: 1,
          candidate_eligible: 1,
          candidate_disqualified: 0,
          output_chunks: 2,
          output_bytes: 256,
          verdict: "pass",
          passed: true,
        },
      ],
    });
  });

  await page.goto("/");

  await page.getByRole("button", { name: "Check Codex Auth" }).click();
  await expect(page.getByText("Auth policy: required")).toBeVisible();

  await page.getByRole("button", { name: "Run Agents" }).click();
  await expect(page.getByText(`run_id=${agentRunId}`)).toBeVisible();
  await expect(page.getByText("status=succeeded")).toBeVisible();
  await expect(page.getByText("report_id=report-e2e")).toBeVisible();

  await page.getByRole("button", { name: "View Latest Report" }).click();
  await expect(page.getByText(/report_id=report-e2e \| generated_at=/)).toBeVisible();
  await expect(page.getByRole("cell", { name: "openai:gpt-5:high" })).toBeVisible();
});
