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

test("jj workflow panel: bootstrap, lane add, and integrate actions call API with expected payloads", async ({
  page,
}) => {
  let bootstrapCalls = 0;
  let laneAddPayload: Record<string, unknown> | null = null;
  let integratePayload: Record<string, unknown> | null = null;

  await page.route("**/tasks", async (route) => {
    if (route.request().method() !== "GET") {
      return route.fallback();
    }
    return respondJson(route, []);
  });

  await page.route("**/orchestrator/jj/bootstrap", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    bootstrapCalls += 1;
    return respondJson(route, {
      command: "scripts/trace-jj.sh bootstrap origin",
      exit_code: 0,
      stdout: "jj-ready",
      stderr: "",
    });
  });

  await page.route("**/orchestrator/jj/lane-add", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    laneAddPayload = JSON.parse(route.request().postData() ?? "{}") as Record<string, unknown>;
    return respondJson(route, {
      command: "scripts/trace-jj.sh lane-add codex-a trunk()",
      exit_code: 0,
      stdout: "lane workspace created",
      stderr: "",
    });
  });

  await page.route("**/orchestrator/jj/integrate", async (route) => {
    if (route.request().method() !== "POST") {
      return route.fallback();
    }
    integratePayload = JSON.parse(route.request().postData() ?? "{}") as Record<string, unknown>;
    return respondJson(route, {
      command: "scripts/trace-jj.sh integrate --base trunk() --good good-a --good good-b --bad bad-a",
      exit_code: 0,
      stdout: "integration complete",
      stderr: "",
    });
  });

  await page.goto("/");

  const jjSection = page.locator("section", { hasText: "JJ Workflow" }).first();

  await jjSection.getByRole("button", { name: "JJ Bootstrap" }).click();
  await expect(jjSection.getByText('"command": "scripts/trace-jj.sh bootstrap origin"')).toBeVisible();
  expect(bootstrapCalls).toBe(1);

  await jjSection.getByRole("button", { name: "Lane Add" }).click();
  await expect.poll(() => laneAddPayload !== null).toBe(true);
  expect(laneAddPayload).toMatchObject({
    lane_name: "codex-a",
    base_revset: "trunk()",
  });

  await jjSection.getByLabel("Good Revisions (comma-separated):").fill("good-a, good-b");
  await jjSection.getByLabel("Bad Revisions (comma-separated):").fill("bad-a");
  await jjSection.getByRole("button", { name: "Integrate" }).click();

  await expect.poll(() => integratePayload !== null).toBe(true);
  expect(integratePayload).toMatchObject({
    base_revset: "trunk()",
    good_revisions: ["good-a", "good-b"],
    bad_revisions: ["bad-a"],
    message: "feat: integrate selected agent revisions",
  });
  await expect(jjSection.getByText("integration complete")).toBeVisible();
});
