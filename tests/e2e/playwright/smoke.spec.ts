import { test, expect } from "@playwright/test";

test("coordinator UI smoke test", async ({ page }) => {
  // Navigate to the coordinator UI
  const baseURL = process.env.COORDINATOR_URL || "http://localhost:8080";
  
  await page.goto(baseURL);
  
  // Verify page title contains "Capsuled Coordinator"
  await expect(page).toHaveTitle(/Capsuled Coordinator/i);
  
  // Check for presence of core UI elements
  await expect(page.locator("nav")).toBeVisible();
  await expect(page.locator("nav h1")).toContainText("Capsuled Coordinator");
  
  // Verify main content area exists
  await expect(page.locator("main")).toBeVisible();
  
  // Check for System Status section
  await expect(page.getByRole("heading", { name: /System Status/i })).toBeVisible();
  
  // Verify status indicator is present
  await expect(page.locator("#status")).toBeVisible();
});
