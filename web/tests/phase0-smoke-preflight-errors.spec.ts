import { expect, test, type Page, type Route } from "@playwright/test";

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

async function installBaselineRoutes(page: Page): Promise<void> {
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
      logged_in: true,
      method: "chatgpt",
      requires_login: false,
      exit_code: 0,
      stdout: "Logged in using ChatGPT",
      stderr: "",
      login_commands: ["codex login"],
    });
  });
}

test("phase0 smoke preflight: invalid target error is surfaced", async ({ page }) => {
  await installBaselineRoutes(page);

  await page.route("**/smoke/runs", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    return respondJson(route, { error: "validate-target preflight failed for trace-smoke:missing" }, 409);
  });

  await page.goto("/");

  const smokeSection = page.locator("section", { hasText: "Smoke Workflow" }).first();
  await smokeSection.getByLabel("Target:").fill("trace-smoke:missing");
  await smokeSection.getByRole("button", { name: "Run Smoke" }).click();

  await expect(smokeSection.getByText(/Smoke workflow failed: .*validate-target/)).toBeVisible();
});

test("phase0 smoke preflight: missing session error is surfaced", async ({ page }) => {
  await installBaselineRoutes(page);

  await page.route("**/smoke/runs", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    return respondJson(route, { error: "status preflight failed for tmux session trace-smoke-missing" }, 409);
  });

  await page.goto("/");

  const smokeSection = page.locator("section", { hasText: "Smoke Workflow" }).first();
  await smokeSection.getByLabel("Session:").fill("trace-smoke-missing");
  await smokeSection.getByRole("button", { name: "Run Smoke" }).click();

  await expect(smokeSection.getByText(/Smoke workflow failed: .*status preflight/)).toBeVisible();
});
