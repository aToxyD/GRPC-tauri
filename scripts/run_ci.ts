import { spawnSync } from "child_process";

const steps = [
  // Generate governance observability reports first (gitignored artifacts
  // required by the FE-165 release gate, e.g. GOVERNANCE_COVERAGE_REPORT.md).
  // Uses the standalone generator entry point: `check:obs` scans invariants
  // before it generates, so on a clean checkout it self-reports FE-165 and
  // exits 1. `check:arch` below enforces FE-165 against the generated artifact.
  { name: "Governance Observability Reports", cmd: "bun", args: ["scripts/governance/observability/index.ts"] },
  { name: "Architectural Checks", cmd: "bun", args: ["run", "check:arch"] },
  { name: "Svelte Type & Diagnostics Checks", cmd: "bun", args: ["run", "check"] },
  { name: "Frontend Unit & Integration Tests", cmd: "bun", args: ["run", "test"] },
  { name: "Build Tauri Binary", cmd: "cargo", args: ["build", "--manifest-path", "src-tauri/Cargo.toml"] },
  { name: "End-to-End Tests", cmd: "bun", args: ["run", "test:e2e"] }
];

console.log("=== STARTING COMPREHENSIVE GOVERNANCE CI GATE ===\n");

let failed = false;

for (const step of steps) {
  console.log(`Running step: ${step.name} (${step.cmd} ${step.args.join(" ")})`);
  const result = spawnSync(step.cmd, step.args, { stdio: "inherit", shell: true });
  if (result.status !== 0) {
    console.error(`\n❌ Step failed: ${step.name}\n`);
    failed = true;
    break;
  }
  console.log(`\n✅ Step succeeded: ${step.name}\n`);
}

if (failed) {
  console.error("❌ GOVERNANCE CI GATE FAILED!");
  process.exit(1);
} else {
  console.log("🎉 ALL GOVERNANCE CI GATES PASSED SUCCESSFULLY!");
  process.exit(0);
}
