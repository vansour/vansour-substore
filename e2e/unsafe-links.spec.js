const { test, expect } = require("@playwright/test");

const fixtureFeedUrl = "http://fixture.invalid:4181/feed";

test("shows field-level validation when saving an unsafe source link", async ({ page }) => {
  await page.goto("/");

  await page.getByLabel("Username").fill("admin");
  await page.getByLabel("Password").fill("admin");
  await page.getByRole("button", { name: "Login" }).click();

  await expect(page.getByText("Signed in as admin")).toBeVisible();

  await page.getByLabel("Create user").fill("unsafe-e2e");
  await page.getByRole("button", { name: "Create" }).click();

  const editor = page.getByPlaceholder(
    "https://example.com/feed\nhttps://news.example.org/article"
  );
  await expect(editor).toBeVisible();

  await editor.fill("http://127.0.0.1/feed");
  await page.getByRole("button", { name: "Save sources" }).click();

  await expect(page.getByText("unsafe target: http://127.0.0.1/feed")).toBeVisible();
  await expect(page.getByText("Request Error")).toHaveCount(0);
});

test("saves a safe source and surfaces diagnostics after cache refresh", async ({ page }) => {
  await page.goto("/");

  await page.getByLabel("Username").fill("admin");
  await page.getByLabel("Password").fill("admin");
  await page.getByRole("button", { name: "Login" }).click();

  await expect(page.getByText("Signed in as admin")).toBeVisible();

  await page.getByLabel("Create user").fill("safe-e2e");
  await page.getByRole("button", { name: "Create" }).click();

  const editor = page.getByPlaceholder(
    "https://example.com/feed\nhttps://news.example.org/article"
  );
  await expect(editor).toBeVisible();

  await editor.fill(fixtureFeedUrl);
  await page.getByRole("button", { name: "Save sources" }).click();

  await expect(page.getByText("Saved links for safe-e2e")).toBeVisible();
  await expect(page.getByText(`unsafe target: ${fixtureFeedUrl}`)).toHaveCount(0);

  await page.getByRole("button", { name: "Refresh cache" }).click();
  await expect(
    page.getByText(/Refreshed cache for safe-e2e with \d+ lines/)
  ).toBeVisible({ timeout: 15_000 });

  await expect(page.getByText("No diagnostics yet")).toHaveCount(0);
  await expect(page.locator(".diagnostic-url").filter({ hasText: fixtureFeedUrl })).toBeVisible();
});
