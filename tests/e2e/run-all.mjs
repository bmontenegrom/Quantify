// Corre todos los smokes E2E (tests/e2e/*.mjs, salvo lib.mjs) en secuencia, cada uno en su
// propio proceso (mismo aislamiento que antes: server + DB temporal por script). Cada script ya
// usa su propio puerto fijo, así que correr en secuencia (no en paralelo) evita pisarse sin
// necesidad de coordinar puertos dinámicos.

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
  if (result.status !== 0) failed = true;
}

process.exitCode = failed ? 1 : 0;
