import { test, expect } from "@playwright/test";
import { spawn, ChildProcess } from "node:child_process";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";
import { setTimeout as delay } from "node:timers/promises";

const PORT = process.env.E2E_PORT ?? "8089";
const BASE = `http://127.0.0.1:${PORT}`;

let server: ChildProcess;
let workdir: string;
let mailbox: string;

test.beforeAll(async () => {
  workdir = await fs.mkdtemp(path.join(os.tmpdir(), "blogrs-e2e-"));
  mailbox = path.join(workdir, "test-mailbox.eml");

  const env = {
    ...process.env,
    PORT,
    DATABASE_URL: `sqlite:${path.join(workdir, "blog.db")}?mode=rwc`,
    BLOG_RS_MAIL: "test",
    BLOG_RS_MAIL_FILE: mailbox,
    BLOG_TOKEN_SECRET: "0123456789abcdef0123456789abcdef0123456789",
    CONFIRM_TOKEN_TTL: "3600",
    OUTBOX_POLL_INTERVAL: "1",
    OUTBOX_BATCH: "32",
    OUTBOX_MAX_ATTEMPTS: "3",
    BLOG_BASE_URL: BASE,
    BLOG_TITLE: "E2E Blog",
    BLOG_FROM: "E2E <noreply@e2e.test>",
    BLOG_ADMIN_EMAIL: "admin@e2e.test",
    BLOG_ADMIN_PASSWORD: "admin-pass-12345",
    SESSION_LIFETIME: "3600",
    RUST_LOG: "info",
  };

  // The binary is built by CI's `cargo build --release -p blog-rs` step.
  // Locally the developer can `cargo build -p blog-rs` first.
  const binary = path.resolve(
    process.cwd(),
    "..", "..", "target",
    process.env.E2E_PROFILE ?? "debug",
    "blog-rs",
  );
  server = spawn(binary, [], { env, stdio: "pipe" });
  server.stdout?.on("data", (b) => process.stdout.write(`[server] ${b}`));
  server.stderr?.on("data", (b) => process.stderr.write(`[server] ${b}`));

  // Wait up to 10s for /healthz to respond 200.
  for (let i = 0; i < 50; i++) {
    try {
      const r = await fetch(`${BASE}/healthz`);
      if (r.ok) return;
    } catch (_) { /* not up yet */ }
    await delay(200);
  }
  throw new Error("server did not become healthy in 10s");
});

test.afterAll(async () => {
  if (server && !server.killed) {
    server.kill("SIGTERM");
    await new Promise<void>((resolve) => server.on("exit", () => resolve()));
  }
  await fs.rm(workdir, { recursive: true, force: true });
});

test("admin can publish a post visible to readers and members get email", async ({ page }) => {
  // 1. Admin login.
  await page.goto(`${BASE}/admin/login`);
  await page.locator('input[name="email"]').fill("admin@e2e.test");
  await page.locator('input[name="password"]').fill("admin-pass-12345");
  await page.locator('button[type="submit"]').click();
  await expect(page).toHaveURL(new RegExp(`^${BASE}/admin/?$`));

  // 2. Create new post.
  await page.goto(`${BASE}/admin/posts/new`);
  await page.locator('input[name="title"]').fill("Hello E2E");
  await page.locator('input[name="slug"]').fill("hello-e2e");
  await page.locator('textarea[name="body_md"]').fill(
    "# Hello\n\nThis is an e2e post.\n\n{{< callout type=\"info\" >}}\nLook at me!\n{{< /callout >}}"
  );
  await page.locator('button[name="action"][value="save"]').click();

  // 3. Publish.
  await page.locator('button[name="action"][value="publish"]').click();

  // 4. Public reader can see the post.
  await page.goto(`${BASE}/`);
  await expect(page.getByRole("heading", { name: "Hello E2E" })).toBeVisible();
  await page.goto(`${BASE}/posts/hello-e2e`);
  await expect(page.locator(".callout")).toContainText("Look at me!");

  // 5. RSS feed includes the post.
  const rss = await (await fetch(`${BASE}/feed.xml`)).text();
  expect(rss).toContain("<title>Hello E2E</title>");
  expect(rss).toContain("/posts/hello-e2e");

  // 6. Member signup (logged out browser context).
  const reader = await page.context().browser()!.newContext();
  const rp = await reader.newPage();
  await rp.goto(`${BASE}/signup`);
  await rp.locator('input[name="email"]').fill("reader@e2e.test");
  await rp.locator('button[type="submit"]').click();
  await expect(rp.getByText(/check your inbox/i)).toBeVisible();

  // 7. Outbox worker writes the confirm email to the test mailbox.
  let mailContents = "";
  for (let i = 0; i < 30; i++) {
    try {
      mailContents = await fs.readFile(mailbox, "utf8");
      if (mailContents.includes("reader@e2e.test") && mailContents.includes("/confirm/")) break;
    } catch (_) { /* not written yet */ }
    await delay(500);
  }
  expect(mailContents).toContain("To: reader@e2e.test");
  expect(mailContents).toMatch(/\/confirm\/[A-Za-z0-9_-]+/);

  // 8. Extract the confirm URL and visit it.
  const confirmUrl = mailContents.match(/https?:\/\/[^\s"<]*\/confirm\/[A-Za-z0-9_-]+/)?.[0];
  expect(confirmUrl).toBeTruthy();
  await rp.goto(confirmUrl!);
  await expect(rp.getByText(/you're subscribed|you are subscribed/i)).toBeVisible();
});
