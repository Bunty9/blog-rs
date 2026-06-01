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
    // figment BLOG_RS__ prefix — double-underscore nesting for nested fields
    BLOG_RS__DATABASE_URL: `sqlite:${path.join(workdir, "blog.db")}?mode=rwc`,
    BLOG_RS__BIND: `127.0.0.1:${PORT}`,
    BLOG_RS__ADMIN_BOOTSTRAP__EMAIL: "admin@e2e.test",
    BLOG_RS__ADMIN_BOOTSTRAP__PASSWORD: "admin-pass-12345",
    // 43-char URL-safe base64 (no padding) → 32 bytes; satisfies signing_key check
    BLOG_RS__SIGNING_KEY: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    // separate site / mail vars (no BLOG_RS__ prefix)
    BLOG_RS_MAIL: "test",
    BLOG_RS_MAIL_FILE: mailbox,
    BLOG_BASE_URL: BASE,
    BLOG_TITLE: "E2E Blog",
    BLOG_FROM: "E2E <noreply@e2e.test>",
    OUTBOX_POLL_INTERVAL: "1",
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
  // The login form submits via JS fetch (doLogin), sets window.location.href
  // = '/admin' on success. Selectors match admin/login.html.
  await page.goto(`${BASE}/admin/login`);
  await page.locator('input[name="email"]').fill("admin@e2e.test");
  await page.locator('input[name="password"]').fill("admin-pass-12345");
  await page.locator('button[type="submit"]').click();
  await expect(page).toHaveURL(new RegExp(`^${BASE}/admin/?$`));

  // 2. Create new post draft.
  // GET /admin/posts/new renders a minimal form; clicking "Create draft"
  // does POST /admin/posts/new which creates the draft and redirects to the
  // editor at /admin/posts/:id.
  await page.goto(`${BASE}/admin/posts/new`);
  await page.getByRole("button", { name: "Create draft" }).click();
  // After redirect we should be on the editor page
  await expect(page).toHaveURL(new RegExp(`${BASE}/admin/posts/\\d+`));

  // 3. Fill title, slug, and body on the editor (posts_edit.html).
  // The editor uses hx-post for auto-save; title/slug/body_md are plain inputs.
  await page.locator('input[name="title"]').fill("Hello E2E");
  await page.locator('input[name="slug"]').fill("hello-e2e");
  await page.locator('textarea[name="body_md"]').fill(
    "# Hello\n\nThis is an e2e post.\n\n{{< callout type=\"info\" >}}\nLook at me!\n{{< /callout >}}"
  );

  // 4. Save: the top-bar "Save now" button is <button type="submit"> inside
  //    the editor form (hx-post="/admin/posts/:id"). Clicking it triggers a
  //    synchronous htmx form submit that saves and swaps #flash-slot.
  await page.getByRole("button", { name: "Save now" }).click();

  // 5. Publish: the "Publish" button is <button type="button" hx-post="...publish"
  //    hx-confirm="...">Publish</button>. It fires an htmx AJAX POST and
  //    triggers a browser confirm dialog via hx-confirm. Accept the dialog.
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "Publish" }).first().click();
  // Wait briefly for htmx to complete the publish request.
  await delay(1000);

  // 6. Public reader can see the post.
  await page.goto(`${BASE}/`);
  await expect(page.getByRole("heading", { name: "Hello E2E" })).toBeVisible();
  await page.goto(`${BASE}/posts/hello-e2e`);
  await expect(page.locator(".callout")).toContainText("Look at me!");

  // 7. RSS feed includes the post.
  const rss = await (await fetch(`${BASE}/feed.xml`)).text();
  expect(rss).toContain("<title>Hello E2E</title>");
  expect(rss).toContain("/posts/hello-e2e");

  // 8. Member signup (logged out browser context).
  // Route is /signup (members router is merged at root, not nested under /members).
  const reader = await page.context().browser()!.newContext();
  const rp = await reader.newPage();
  await rp.goto(`${BASE}/signup`);
  await rp.locator('input[name="email"]').fill("reader@e2e.test");
  await rp.locator('button[type="submit"]').click();
  await expect(rp.getByText(/check your inbox/i)).toBeVisible();

  // 9. Outbox worker writes the confirm email to the test mailbox.
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

  // 10. Extract the confirm URL and visit it.
  const confirmUrl = mailContents.match(/https?:\/\/[^\s"<]*\/confirm\/[A-Za-z0-9_-]+/)?.[0];
  expect(confirmUrl).toBeTruthy();
  await rp.goto(confirmUrl!);
  // confirm_done.html: <h1>You're subscribed</h1>
  await expect(rp.getByText(/you're subscribed|you are subscribed/i)).toBeVisible();
});
