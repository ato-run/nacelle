import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e/playwright",
  timeout: 30 * 1000,
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    trace: "on-first-retry",
    baseURL: "http://localhost:8080",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  // Web server configuration for automated testing
  // This starts a lightweight test server that serves the coordinator UI
  webServer: {
    command: "node tests/e2e/playwright/test-server.js",
    port: 8080,
    reuseExistingServer: !process.env.CI,
    timeout: 10 * 1000,
  },
});
