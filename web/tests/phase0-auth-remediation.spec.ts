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

test("phase0 auth remediation: required policy blocks smoke run with actionable command", async ({
  page,
}) => {
  let smokeStartCalled = false;

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

  await page.route("**/smoke/runs", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    smokeStartCalled = true;
    return respondJson(route, { error: "unexpected smoke start" }, 500);
  });

  await page.goto("/");

  await page.getByRole("button", { name: "Check Codex Auth" }).click();
  await expect(page.getByText("Run one of: codex login | codex login --device-auth")).toBeVisible();

  const smokeSection = page.locator("section", { hasText: "Smoke Workflow" }).first();
  await smokeSection.getByRole("button", { name: "Run Smoke" }).click();

  await expect(
    smokeSection.getByText("Smoke workflow failed: Codex auth required before smoke-run. Run: codex login"),
  ).toBeVisible();
  expect(smokeStartCalled).toBe(false);
});
