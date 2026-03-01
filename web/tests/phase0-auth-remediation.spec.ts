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

test("phase0 auth remediation: required policy blocks agent run with actionable command", async ({
  page,
}) => {
  let agentStartCalled = false;

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
    return respondJson(route, {
      command: "codex login status",
      policy: "required",
      available: true,
      logged_in: false,
      method: null,
      requires_login: true,
      exit_code: 1,
      stdout: "",
      stderr: "not logged in",
      login_commands: ["codex login", "codex login --device-auth"],
    });
  });

  await page.route("**/agent/runs", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    agentStartCalled = true;
    return respondJson(route, { error: "unexpected smoke start" }, 500);
  });

  await page.goto("/");

  await page.getByRole("button", { name: "Check Codex Auth" }).click();
  await expect(page.getByText("Run one of: codex login | codex login --device-auth")).toBeVisible();

  const agentSection = page.locator("section", { hasText: "Agent Runs" }).first();
  await agentSection.getByRole("button", { name: "Run Agents" }).click();

  await expect(
    agentSection.getByText("Agent run failed: Codex auth required before agent-run. Run: codex login"),
  ).toBeVisible();
  expect(agentStartCalled).toBe(false);
});
