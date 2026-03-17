const { test, expect } = require("@playwright/test");

test("reorders users from the dashboard and persists the updated order", async ({ page }) => {
  const suffix = Date.now();
  const firstUser = `order-a-${suffix}`;
  const secondUser = `order-b-${suffix}`;

  await page.goto("/");

  await page.getByLabel("Username").fill("admin");
  await page.getByLabel("Password").fill("admin");
  await page.getByRole("button", { name: "Login" }).click();

  await expect(page.getByText("Signed in as admin")).toBeVisible();

  await page.getByLabel("Create user").fill(firstUser);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByText(`Created ${firstUser}`)).toBeVisible();

  await page.getByLabel("Create user").fill(secondUser);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByText(`Created ${secondUser}`)).toBeVisible();

  const userNames = page.locator(".user-list .user-card strong");
  await expect(page.locator(".user-list .user-card").filter({ hasText: firstUser })).toHaveCount(1);
  await expect(page.locator(".user-list .user-card").filter({ hasText: secondUser })).toHaveCount(1);
  await expect
    .poll(async () => relativeOrder(userNames, firstUser, secondUser))
    .toBeLessThan(0);

  await page
    .locator(".user-list .user-card")
    .filter({ hasText: firstUser })
    .getByRole("button", { name: "Down" })
    .click();

  await expect(page.getByText("Updated user order")).toBeVisible();
  await expect
    .poll(async () => relativeOrder(userNames, firstUser, secondUser))
    .toBeGreaterThan(0);

  await page
    .locator(".user-list .user-card")
    .filter({ hasText: firstUser })
    .getByRole("button", { name: "Up" })
    .click();

  await expect(page.getByText("Updated user order")).toBeVisible();
  await expect
    .poll(async () => relativeOrder(userNames, firstUser, secondUser))
    .toBeLessThan(0);
});

async function relativeOrder(userNames, firstUser, secondUser) {
  const names = await userNames.allTextContents();
  return names.indexOf(firstUser) - names.indexOf(secondUser);
}
