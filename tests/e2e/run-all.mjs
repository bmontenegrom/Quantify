
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = dirname(fileURLToPath(import.meta.url));

const scripts = readdirSync(DIR)
  .filter((f) => f.endsWith(".mjs") && f !== "lib.mjs" && f !== "run-all.mjs")
  .sort();

console.log(`Corriendo ${scripts.length} smokes E2E: ${scripts.join(", ")}`);

let failed = false;
for (const script of scripts) {
  console.log(`\n=== ${script} ===`);
  const result = spawnSync(process.execPath, [join(DIR, script)], {
    stdio: "inherit",
    env: process.env,
  });
  if (result.status !== 0) {
    failed = true;
    console.error(`✗ ${script} terminó con status=${result.status} signal=${result.signal} error=${result.error?.message}`);
  }
}

process.exitCode = failed ? 1 : 0;
