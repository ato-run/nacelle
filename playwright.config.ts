import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e/playwright",
  timeout: 30 * 1000,
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    trace: "on-first-retry",
    baseURL: process.env.COORDINATOR_URL || "http://localhost:8080",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  // Web server configuration for local testing
  // The coordinator must be started separately for these tests to pass
  // Run: cd client && go run ./cmd/client/main.go -config test-config.yaml
});
