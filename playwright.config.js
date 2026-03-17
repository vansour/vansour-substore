const { defineConfig } = require("@playwright/test");

const port = 4173;
const upstreamPort = 4181;
const webDistDir = "target/dx/submora-web/release/web/public";
const upstreamOverride = `fixture.invalid:${upstreamPort}=127.0.0.1:${upstreamPort}`;

module.exports = defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  fullyParallel: false,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    headless: true,
    trace: "on-first-retry",
  },
  webServer: [
    {
      command: `UPSTREAM_PORT=${upstreamPort} node e2e/fixtures/upstream-server.js`,
      url: `http://127.0.0.1:${upstreamPort}/healthz`,
      reuseExistingServer: !process.env.CI,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 30_000,
    },
    {
      command: [
        "rm -rf .tmp/e2e",
        "mkdir -p .tmp/e2e",
        "dx build --platform web --package submora-web --release",
        [
          "HOST=127.0.0.1",
          `PORT=${port}`,
          `WEB_DIST_DIR=${webDistDir}`,
          "DATABASE_URL=sqlite://.tmp/e2e/substore.db?mode=rwc",
          "COOKIE_SECURE=false",
          "SESSION_TTL_MINUTES=60",
          "SESSION_CLEANUP_INTERVAL_SECS=60",
          "LOGIN_MAX_ATTEMPTS=5",
          "LOGIN_WINDOW_SECS=60",
          "LOGIN_LOCKOUT_SECS=300",
          "CACHE_TTL_SECS=300",
          "DB_MAX_CONNECTIONS=1",
          "FETCH_TIMEOUT_SECS=5",
          "DNS_CACHE_TTL_SECS=30",
          `FETCH_HOST_OVERRIDES=${upstreamOverride}`,
          "CONCURRENT_LIMIT=4",
          "MAX_LINKS_PER_USER=20",
          "MAX_USERS=20",
          "ADMIN_USER=admin",
          "ADMIN_PASSWORD=admin",
          "CORS_ALLOW_ORIGIN=http://127.0.0.1:8081,http://localhost:8081",
          "cargo run -p submora",
        ].join(" "),
      ].join(" && "),
      url: `http://127.0.0.1:${port}/healthz`,
      reuseExistingServer: !process.env.CI,
      stdout: "pipe",
      stderr: "pipe",
      timeout: 300_000,
    },
  ],
});
