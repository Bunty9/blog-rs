import { defineConfig, devices } from "@playwright/test";

const PORT = process.env.E2E_PORT ?? "8089";
const BASE_URL = process.env.E2E_BASE_URL ?? `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: ".",
  fullyParallel: false, // one server, sequential tests
  retries: 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  timeout: 60_000,
  use: {
    baseURL: BASE_URL,
    actionTimeout: 10_000,
    navigationTimeout: 30_000,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    // The server is brought up by the spec via a child_process so we can set
    // env vars per-test. We deliberately do NOT use Playwright's webServer
    // option for this; leaving the field commented for posterity.
  },
});
