/**
 * Patches svelte-spa-router's package.json to add a default export condition
 * so bun test can resolve the module (vitest uses Vite+Svelte plugin which
 * understands the "svelte" condition natively).
 */
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "fs";
import { join, dirname } from "path";

const pkgPath = join(import.meta.dir, "..", "node_modules", "svelte-spa-router", "package.json");
const indexPath = join(dirname(pkgPath), "dist", "index.js");

// Create the stub entry point if it doesn't exist
if (!existsSync(indexPath)) {
  mkdirSync(dirname(indexPath), { recursive: true });
  writeFileSync(
    indexPath,
    `export async function push(location) {
  // Stub for bun test — real implementation uses history.pushState via Router.svelte
}
`,
    "utf-8",
  );
  console.log("Created svelte-spa-router/dist/index.js stub");
}

// Read and patch package.json
const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));

const mainExport = pkg.exports?.["."];
if (mainExport && !mainExport.default) {
  mainExport.default = "./dist/index.js";
  mainExport.import = "./dist/index.js";
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n", "utf-8");
  console.log("Patched svelte-spa-router package.json — added default/import export conditions");
} else {
  console.log("svelte-spa-router already patched");
}
