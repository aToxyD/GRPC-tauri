import { spawnSync } from "child_process";

const steps = [
  { name: "Architectural Checks", cmd: "bun", args: ["run", "check:arch"] },
  { name: "Svelte Type & Diagnostics Checks", cmd: "bun", args: ["run", "check"] },
  { name: "Frontend Unit & Integration Tests", cmd: "bun", args: ["run", "test"] },
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
