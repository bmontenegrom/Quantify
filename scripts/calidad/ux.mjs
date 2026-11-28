import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(join(process.cwd(), "package.json"));
const { chromium } = require("playwright");
const { AxeBuilder } = require("@axe-core/playwright");
const lighthouse = (await import(pathToFileURL(require.resolve("lighthouse")).href)).default;
const desktop = (await import(pathToFileURL(require.resolve("lighthouse/core/config/desktop-config.js")).href)).default;

const [base, out] = process.argv.slice(2);
const RUNS = 3;
const PORT = 9333;
const USERS = {
  estudiante: { email: "estudiante@quantify.local", password: "estudiante123", views: [] },
  docente: {
    email: "docente@quantify.local",
    password: "docente123",
    views: ["submissions", "gradebook", "students", "groups", "courses", "users", "practices", "instruments"],
  },
};
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

async function axe(page) {
  const result = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  return result.violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.length, help: v.help }));
}

async function audit(name) {
  const runs = [];
  for (let i = 0; i < RUNS; i++) {
    const { lhr } = await lighthouse(base, { port: PORT, output: "json", logLevel: "error", disableStorageReset: true }, desktop);
    runs.push(Object.fromEntries(Object.entries(lhr.categories).map(([k, c]) => [k, Math.round(c.score * 100)])));
  }
  const categories = Object.keys(runs[0]);
  return { page: name, runs, median: Object.fromEntries(categories.map((k) => [k, median(runs.map((r) => r[k]))])) };
}

async function session() {
  const dir = mkdtempSync(join(tmpdir(), "quantify-ux-"));
  const context = await chromium.launchPersistentContext(dir, {
    viewport: { width: 1350, height: 940 },
    args: [`--remote-debugging-port=${PORT}`],
  });
  const page = context.pages()[0] ?? (await context.newPage());
  await page.goto(base, { waitUntil: "networkidle" });
  return { context, page, close: async () => (await context.close(), rmSync(dir, { recursive: true, force: true })) };
}

const axeResults = {};
const lighthouseResults = [];

const anon = await session();
axeResults.login = await axe(anon.page);
lighthouseResults.push(await audit("login"));
await anon.close();

for (const [role, user] of Object.entries(USERS)) {
  const s = await session();
  await s.page.fill('#login-form input[name="email"]', user.email);
  await s.page.fill('#login-form input[name="password"]', user.password);
  await s.page.click('#login-form button[type="submit"]');
  await s.page.waitForSelector("#app-shell:not(.hidden)");
  await s.page.waitForLoadState("networkidle");
  axeResults[`${role}/inicio`] = await axe(s.page);
  for (const view of user.views) {
    await s.page.click(`.tab.teacher-only[data-view="${view}"]`);
    await s.page.waitForLoadState("networkidle");
    axeResults[`${role}/${view}`] = await axe(s.page);
  }
  lighthouseResults.push(await audit(`${role}/inicio`));
  await s.close();
}

writeFileSync(join(out, "axe.json"), JSON.stringify(axeResults, null, 1));
writeFileSync(join(out, "lighthouse.json"), JSON.stringify(lighthouseResults, null, 1));
