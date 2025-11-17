import { test, expect } from "@playwright/test";

test("dummy smoke", async ({ page }) => {
  await page.goto("https://example.com");
  await expect(page).toHaveTitle(/Example/i);
});
