/**
 * Release Certification Script
 *
 * Verifies all governance gates before a production release.
 * Must exit 0 for a release to be certified.
 *
 * Usage: bun run scripts/release_certification.ts
 *
 * @see docs/releases/release_certification_process.md
 */

import { execSync } from "child_process";
import { readFileSync, existsSync } from "fs";
import { Glob } from "bun";

const colors = {
    red: "\x1b[31m",
    green: "\x1b[32m",
    yellow: "\x1b[33m",
    cyan: "\x1b[36m",
    reset: "\x1b[0m",
    bold: "\x1b[1m",
};

type CertResult = {
    gate: string;
    status: "PASS" | "FAIL" | "SKIP";
    output: string;
};

const results: CertResult[] = [];

let exitCode = 0;

function runGate(
    name: string,
    command: string,
    options: { cwd?: string; timeout?: number } = {}
): void {
    process.stdout.write(`  ${name}... `);
    try {
        const output = execSync(command, {
            encoding: "utf-8",
            stdio: ["ignore", "pipe", "pipe"],
            cwd: options.cwd,
            timeout: options.timeout ?? 120000,
        });
        results.push({ gate: name, status: "PASS", output: output.trim() });
        console.log(`${colors.green}PASS${colors.reset}`);
    } catch (e: any) {
        results.push({
            gate: name,
            status: "FAIL",
            output: (e.stderr ?? e.stdout ?? e.message ?? "").trim(),
        });
        console.log(`${colors.red}FAIL${colors.reset}`);
    }
}

console.log(`${colors.bold}\nRelease Certification Suite${colors.reset}`);
console.log(`Date: ${new Date().toISOString()}`);
console.log(`Commit: ${execSync("git rev-parse HEAD", { encoding: "utf-8" }).trim()}\n`);

// ── Gate 1: Rust Tests ────────────────────────────────────────
console.log(`\n${colors.cyan}Gate 1 — Backend Tests${colors.reset}`);
runGate("cargo test", "cargo test", { cwd: "src-tauri", timeout: 600000 });

// ── Gate 2: Clippy ────────────────────────────────────────────
console.log(`\n${colors.cyan}Gate 2 — Linting${colors.reset}`);
runGate("cargo clippy", "cargo clippy --all-targets --all-features -- -D warnings", {
    cwd: "src-tauri",
    timeout: 300000,
});

// ── Gate 3: Formatting ─────────────────────────────────────────
console.log(`\n${colors.cyan}Gate 3 — Formatting${colors.reset}`);
runGate("cargo fmt --check", "cargo fmt --check", { cwd: "src-tauri" });

// ── Gate 4: Architecture Audit ─────────────────────────────────
console.log(`\n${colors.cyan}Gate 4 — Architecture Audit${colors.reset}`);
runGate("bun run check:arch", "bun run scripts/check_arch.ts", { timeout: 120000 });

// ── Gate 5: Determinism Suite ──────────────────────────────────
console.log(`\n${colors.cyan}Gate 5 — Determinism Suite${colors.reset}`);
runGate("reporting_reproducibility", "cargo test reporting_reproducibility 2>&1 | tail -3", {
    cwd: "src-tauri",
    timeout: 120000,
});
runGate("recovery_determinism", "cargo test recovery_determinism 2>&1 | tail -3", {
    cwd: "src-tauri",
    timeout: 120000,
});

// ── Gate 6: No Undocumented Exceptions ─────────────────────────
console.log(`\n${colors.cyan}Gate 6 — Exception Governance${colors.reset}`);

let exceptionIssues = 0;
const exceptionOutput: string[] = [];

// Check that adr_exception_registry.md exists
if (!existsSync("docs/architecture/adr_exception_registry.md")) {
    exceptionIssues++;
    exceptionOutput.push("MISSING: docs/architecture/adr_exception_registry.md");
}

// Check that all [arch:allow-*] tags have ADR references
const glob = new Glob("src-tauri/src/**/*.rs");
for (const file of glob.scanSync(".")) {
    const content = readFileSync(file, "utf-8");
    const lines = content.split("\n");
    for (let i = 0; i < lines.length; i++) {
        if (lines[i].includes("[arch:allow-") && !lines[i].includes("see ADR-")) {
            // Check adjacent lines
            const hasADR =
                (i > 0 && lines[i - 1].includes("see ADR-")) ||
                (i + 1 < lines.length && lines[i + 1].includes("see ADR-"));
            if (!hasADR) {
                exceptionIssues++;
                exceptionOutput.push(
                    `NO_ADR_REF: ${file}:${i + 1} — ${lines[i].trim().substring(0, 80)}`
                );
            }
        }
    }
}

if (exceptionIssues === 0) {
    results.push({ gate: "Undocumented exceptions", status: "PASS", output: "0 issues" });
    console.log(`  ${colors.green}PASS${colors.reset} — No undocumented exceptions`);
} else {
    results.push({
        gate: "Undocumented exceptions",
        status: "FAIL",
        output: exceptionOutput.join("\n"),
    });
    console.log(`  ${colors.red}FAIL${colors.reset} — ${exceptionIssues} issue(s)`);
    exceptionOutput.forEach((o) => console.log(`    ${o}`));
}

// ── Gate 7: No Expired ADR Exceptions ──────────────────────────
console.log(`\n${colors.cyan}Gate 7 — Exception Expiry${colors.reset}`);

let expiredCount = 0;
const expiredOutput: string[] = [];

if (existsSync("docs/architecture/adr_exception_registry.md")) {
    const registry = readFileSync("docs/architecture/adr_exception_registry.md", "utf-8");
    const today = new Date();
    const expiryRegex = /\|\s*(\d{4}-\d{2}-\d{2})\s*\|\s*(Architecture|Sync|Security|Infrastructure)\s*\|/g;
    let match;
    while ((match = expiryRegex.exec(registry)) !== null) {
        const expiryDate = new Date(match[1] + "T23:59:59Z");
        if (expiryDate < today) {
            expiredCount++;
            // Find the line context
            const lineIndex = registry.substring(0, match.index).split("\n").length;
            expiredOutput.push(`EXPIRED: line ${lineIndex} — expired ${match[1]}`);
        }
    }
}

if (expiredCount === 0) {
    results.push({ gate: "Exception expiry", status: "PASS", output: "No expired exceptions" });
    console.log(`  ${colors.green}PASS${colors.reset} — No expired exceptions`);
} else {
    results.push({
        gate: "Exception expiry",
        status: "FAIL",
        output: expiredOutput.join("\n"),
    });
    console.log(`  ${colors.red}FAIL${colors.reset} — ${expiredCount} expired exception(s)`);
    expiredOutput.forEach((o) => console.log(`    ${o}`));
}

// ── Gate 8: No Expired ADR Exemptions in check_arch.ts comments ──
// (Already covered by check_arch.ts Rule 123)

// ── Summary ────────────────────────────────────────────────────
console.log(`\n${colors.bold}${colors.cyan}═══════════════════════════════════════${colors.reset}`);
console.log(`${colors.bold}${colors.cyan}  RELEASE CERTIFICATION SUMMARY${colors.reset}`);
console.log(`${colors.bold}${colors.cyan}═══════════════════════════════════════${colors.reset}\n`);

const passCount = results.filter((r) => r.status === "PASS").length;
const failCount = results.filter((r) => r.status === "FAIL").length;

for (const result of results) {
    const icon = result.status === "PASS" ? `${colors.green}✅${colors.reset}` : `${colors.red}❌${colors.reset}`;
    console.log(`  ${icon} ${result.gate}: ${result.status}`);
}

console.log(`\n  ${colors.bold}Results:${colors.reset} ${passCount} passed, ${failCount} failed`);
console.log(`  ${colors.bold}Timestamp:${colors.reset} ${new Date().toISOString()}`);
console.log(
    `  ${colors.bold}Commit:${colors.reset} ${execSync("git rev-parse HEAD", { encoding: "utf-8" }).trim()}`
);

// Generate machine-readable output
const summary = {
    certified: failCount === 0,
    timestamp: new Date().toISOString(),
    commit: execSync("git rev-parse HEAD", { encoding: "utf-8" }).trim(),
    results: results.map((r) => ({ gate: r.gate, status: r.status })),
};

const outputFile = `target/release_certification_${Date.now()}.json`;
try {
    const { writeFileSync, mkdirSync } = await import("fs");
    mkdirSync("target", { recursive: true });
    writeFileSync(outputFile, JSON.stringify(summary, null, 2));
    console.log(`  ${colors.bold}Report:${colors.reset} ${outputFile}`);
} catch {
    // target directory may not exist
}

console.log(`\n${colors.bold}Certification:${colors.reset} ${
    failCount === 0 ? `${colors.green}CERTIFIED${colors.reset}` : `${colors.red}NOT CERTIFIED${colors.reset}`
}\n`);

process.exit(failCount > 0 ? 1 : 0);
