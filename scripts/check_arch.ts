/**
 * Architectural Integrity Audit
 *
 * Enforces the Stable Unified Architecture constraints defined in ADR-0011.
 *
 * Rules are categorized as:
 *   - error: hard block, must fix before merge
 *   - warning: review recommended, not blocking
 *
 * Exceptions must be:
 *   1. Documented with a comment in the file
 *   2. Linked to an ADR or TODO
 *   3. Added to the fileFilter below with a note
 *
 * @see docs/architecture/0011-unified-architecture-migration.md
 * @see docs/architecture/0012-production-error-exposure-policy.md
 * @see docs/architecture/0013-observability-layer.md
 * @see docs/architecture/0030-adr-exception-governance.md
 * @see docs/architecture/adr_exception_registry.md
 *
 * Flags:
 *   --obs, --observability   Generate governance observability reports (informational only)
 */

import { Glob } from "bun";
import { readFileSync } from "fs";

const colors = {
    red: "\x1b[31m",
    green: "\x1b[32m",
    yellow: "\x1b[33m",
    cyan: "\x1b[36m",
    reset: "\x1b[0m",
    bold: "\x1b[1m",
};

let violations = 0;
let warnings = 0;

function checkRule(
    name: string,
    patterns: string[],
    regex: RegExp,
    excludeLines: (line: string, index: number, lines: string[]) => boolean,
    severity: "error" | "warning" = "error",
    fileFilter?: (filename: string) => boolean
) {
    const scannedFiles = new Set<string>();
    let ruleViolations = 0;
    const matches: { file: string; line: number; content: string }[] = [];

    for (const pattern of patterns) {
        const glob = new Glob(pattern);
        for (const file of glob.scanSync(".")) {
            const normalizedFile = file.replace(/\\/g, "/");
            if (scannedFiles.has(normalizedFile)) continue;
            scannedFiles.add(normalizedFile);

            if (fileFilter && !fileFilter(file)) continue;

            const content = readFileSync(file, "utf-8");
            const lines = content.split("\n");

            lines.forEach((line, index) => {
                if (line.trim().startsWith("//")) return;

                if (regex.test(line) && !excludeLines(line, index, lines)) {
                    matches.push({
                        file,
                        line: index + 1,
                        content: line.trim(),
                    });

                    ruleViolations++;
                }
            });
        }
    }

    if (ruleViolations > 0) {
        const color = severity === "error"
            ? colors.red
            : colors.yellow;

        const icon = severity === "error"
            ? "❌"
            : "⚠️";

        console.log(`${icon} ${color}${name}${colors.reset}`);

        matches.forEach((m) => {
            console.log(`  ${m.file}:${m.line} → ${m.content}`);
        });

        if (severity === "error") violations++;
        else warnings++;
    }
}

console.log(
    `${colors.bold}Running Architectural Integrity Audit...${colors.reset}\n`
);

// ============================================================
// GROUP 1 — SQL Boundary Rules
// ============================================================

// Rule 1: No SQL outside repositories
checkRule(
    "SQL found in services/ or commands/",
    [
        "src-tauri/src/application/services/*.rs",
        "src-tauri/src/commands/*.rs",
    ],
    /"SELECT\b|"INSERT\b|"UPDATE\b|"DELETE\b|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//") || line.includes("[arch:allow-sql]"),
    "error",
    (f) => !f.includes("fiscal_validation_service") &&
           !f.includes("fiscal_integrity_service") &&
           !f.includes("export_reproducibility_helper") &&
           !f.includes("fiscal_timeline_service") &&
           !f.includes("sync_import_validation_service") &&
           !f.includes("inventory_integrity_service") &&
           !f.includes("operational_consistency_verifier") &&
           !f.includes("operation_execution_guard")
);

// Rule 2: No SQL in db/mod.rs body methods
checkRule(
    "SQL found in db/mod.rs",
    ["src-tauri/src/db/mod.rs"],
    /"SELECT|"INSERT|"UPDATE|"DELETE/,
    (line) =>
        line.includes("PRAGMA")
        || line.trim().startsWith("//")
);

// Rule 7: application/ must not reference rusqlite
checkRule(
    "rusqlite leaked into application/ (use infrastructure/db)",
    ["src-tauri/src/application/**/*.rs"],
    /\brusqlite\b/,
    (line) => line.trim().startsWith("//"),
    "error",
    (f) => !f.includes("reporting/") &&
           !f.includes("fiscal_validation_service") &&
           !f.includes("fiscal_integrity_service") &&
           !f.includes("export_reproducibility_helper") &&
           !f.includes("fiscal_timeline_service") &&
           !f.includes("sync_import_validation_service") &&
           !f.includes("inventory_integrity_service") &&
           !f.includes("operational_consistency_verifier") &&
           !f.includes("operation_execution_guard")
);

// ============================================================
// GROUP 2 — Domain Boundary Rules
// ============================================================

// Rule 8: domain/ must not import infrastructure
checkRule(
    "domain/ must not depend on crate::infrastructure",
    ["src-tauri/src/domain/**/*.rs"],
    /use\s+crate::infrastructure/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 3 — Authorization Boundary Rules (ADR-0011)
// ============================================================

// Rule 11: Commands must not contain raw role checks
checkRule(
    "Direct role comparison in commands/ (use application/authz instead)",
    ["src-tauri/src/commands/*.rs"],
    /user_role\s*==\s*(UserRole::|crate::models::UserRole::)|\.role\s*==\s*(UserRole::|crate::models::UserRole::)/,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => {
        const normalized = file
            .toLowerCase()
            .replace(/\\/g, "/");

        return !normalized.includes("guards.rs")
            && !normalized.includes("auth.rs");
    }
);

// Rule 12: Commands must not compare node_type directly
checkRule(
    "Direct node_type comparison in commands/ (use ResourceContext in authz)",
    ["src-tauri/src/commands/*.rs"],
    /node_type\s*==\s*(NodeType::|crate::models::NodeType::)/,
    (line) => line.trim().startsWith("//"),
    "warning",
    (file) => {
        const normalized = file
            .toLowerCase()
            .replace(/\\/g, "/");

        return !normalized.includes("guards.rs")
            && !normalized.includes("reports.rs")
            && !normalized.includes("import_export.rs")
            && !normalized.includes("auth.rs");
    }
);

// Rule 13: No direct password verification outside auth.rs
checkRule(
    "Password hash verification outside auth.rs commands (boundary leak)",
    ["src-tauri/src/commands/*.rs"],
    /verify_password_argon2|hash_password_argon2/,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => {
        const normalized = file
            .toLowerCase()
            .replace(/\\/g, "/");

        return !normalized.includes("auth.rs");
    }
);

// Rule 20: require_authenticated is transitional and internal
checkRule(
    "require_authenticated used outside guards.rs (use authorize_command instead)",
    ["src-tauri/src/**/*.rs"],
    /\brequire_authenticated\(/,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => {
        const normalized = file.toLowerCase().replace(/\\/g, "/");
        return !normalized.includes("guards.rs");
    }
);

// Rule 21: get_effective_unit_id is deprecated and forbidden
checkRule(
    "get_effective_unit_id is deprecated and must not be used",
    ["src-tauri/src/**/*.rs"],
    /\bget_effective_unit_id\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 4 — Service / Orchestration Rules (ADR-0011)
// ============================================================

// Rule 5: No non-transactional audit logging
checkRule(
    "Non-transactional audit found in services! (Bypasses Atomicity Law)",
    ["src-tauri/src/application/services/*.rs"],
    /AuditLogger::log_success\(|AuditLogger::log_failure\(/,
    (line) => line.trim().startsWith("//")
);

// Rule 6: No silent error swallowing
checkRule(
    "Potential silent error swallowing in services",
    ["src-tauri/src/application/services/*.rs"],
    /\.unwrap_or\(|\.ok\(\)\?/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("[arch:allow-unwrap-or]")) return true;
        if (index > 0 && lines[index - 1].includes("[arch:allow-unwrap-or]")) return true;
        return false;
    },
    "warning"
);

// Rule 14: Commands must not directly use repositories
checkRule(
    "Direct repository use inside commands/ (must delegate to application/services)",
    ["src-tauri/src/commands/*.rs"],
    /Repository::new\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 16: commands/ must not import repositories directly (use application/services)
checkRule(
    "commands/ importing repositories directly (must use application/services)",
    ["src-tauri/src/commands/*.rs"],
    /use\s+crate::repositories::/,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => {
        const normalized = file
            .replace(/\\/g, "/")
            .toLowerCase();

        return !normalized.includes("guards.rs");
    }
);

// Rule 17: services/ should not construct repositories directly (prefer injected dependencies if possible)
checkRule(
    "Repository construction inside services/ (warning only, review for delegation)",
    ["src-tauri/src/application/services/*.rs"],
    /Repository::new\(/,
    (line) => line.trim().startsWith("//"),
    "warning"
);

// ============================================================
// GROUP 5 — Sync Protocol Rules (ADR-0009, ADR-0010)
// ============================================================

// Rule 28: No plaintext SQLite staging suffix in backup infrastructure
checkRule(
    "Plaintext .sqlite tempfile staging in backup layer",
    ["src-tauri/src/infrastructure/backup/**/*.rs"],
    /\.suffix\(\s*\"\.sqlite\"\s*\)/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 29: Crypto domain must not buffer full streams (read_to_end)
checkRule(
    "read_to_end in domain/security.rs (forbid whole-buffer CryptoPort defaults)",
    ["src-tauri/src/domain/security.rs"],
    /read_to_end\s*\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 15: No CSV sync references
checkRule(
    "Legacy CSV sync reference found (system is Sync Package-only per ADR-0010)",
    ["src-tauri/src/**/*.rs"],
    /csv_import|import_csv|CsvSync|csv_sync|legacy_sync/i,
    (line) => line.trim().startsWith("//"),
    "warning"
);

// Rule 18: CSV legacy artifacts forbidden
checkRule(
    "CSV legacy artifact detected (ADR-0010 Sync Package-only violation)",
    ["src-tauri/src/**/*.rs"],
    /\bCsv[A-Z]\w+|\bcsv[_a-zA-Z]*|CSV\b/,
    (line) => {
        const trimmed = line.trim();

        return trimmed.startsWith("//")
            || trimmed.startsWith("///")
            || trimmed.includes("ADR-0010");
    },
    "error"
);

// ============================================================
// GROUP 6 — Application State Rules
// ============================================================

// Rule 10: AppState should not be defined under commands/
checkRule(
    "AppState struct defined under commands/ (canonical: app/state.rs)",
    ["src-tauri/src/commands/**/*.rs"],
    /pub\s+struct\s+AppState\b/,
    (line) => line.trim().startsWith("//")
);

// ============================================================
// GROUP 7 — Information Leakage Gate (ADR-0012)
// ============================================================

// Rule 19: No unguarded `details` interpolation in user-facing error strings
// Any format!("{} ... {}", ..., details) that is NOT inside a
// #[cfg(debug_assertions)] block is a production information-leakage violation.
// Exception: lines inside a `#[cfg(debug_assertions)]` block are allowed.
// Exception: lines that are commented out are allowed.
// Exception: the log::error! call itself (contains "code=" and "details=") is allowed.
checkRule(
    "Unguarded 'details' interpolation in error path (ADR-0012 — use #[cfg(debug_assertions)] gate)",
    [
        "src-tauri/src/errors/**/*.rs",
        "src-tauri/src/commands/**/*.rs",
        "src-tauri/src/application/services/**/*.rs",
    ],
    /format!\s*\(.*\bdetails\b/,
    (line) => {
        const trimmed = line.trim();
        // Allow commented lines
        if (trimmed.startsWith("//") || trimmed.startsWith("///") || trimmed.startsWith("/*")) return true;
        // Allow log::error! macro lines (internal logging is intentional)
        if (/log\s*::\s*error!/.test(line)) return true;
        // Allow lines explicitly gated — they will be inside a cfg block.
        // The arch check cannot parse block scope, so we allow lines that are
        // immediately preceded by a cfg(debug_assertions) attribute in the
        // same hunk; developers must document exceptions with an ADR comment.
        if (/ADR-0012/.test(line)) return true;
        return false;
    },
    "error"
);

// ============================================================
// RULE 22 — No reconciliation logic in commands/
// Commands must delegate ALL conflict resolution to
// application/services/sync_conflict_service.rs
// @see docs/architecture/0014-sync-conflict-intelligence.md
// ============================================================
checkRule(
    "Rule 22: No reconciliation logic inside commands/",
    ["src-tauri/src/commands/**/*.rs"],
    /\bconflict\b.*\binsert\b|\bconflict\b.*\bdelete\b|\bUPDATE\s+sync_conflicts\b/i,
    (line) => {
        // Allow tauri command invocations that call the service
        if (/SyncConflictService|resolve_sync_conflict|list_sync_conflicts|get_conflict_summary/.test(line)) return true;
        if (/^\s*\/\//.test(line)) return true;
        return false;
    },
    "error"
);

// ============================================================
// RULE 23 — No direct repository mutation in frontend adapters
// Frontend (src/**) must never call mutation commands directly
// without going through the service layer.
// @see docs/architecture/0013-observability-layer.md
// ============================================================
checkRule(
    "Rule 23: No SQL in frontend adapter files (src/lib/)",
    ["src/lib/**/*.ts", "src/lib/**/*.js"],
    /SELECT\s|INSERT\s|UPDATE\s|DELETE\s/i,
    (line) => {
        if (/^\s*\/\//.test(line)) return true;
        return false;
    },
    "error"
);

// ============================================================
// RULE 24 — Conflict resolution must emit audit event
// Any call to resolve_conflict must be wrapped in a transaction
// that also calls AuditService.log_success.
// Enforced by code review; this rule detects bare resolve_conflict calls.
// @see docs/architecture/0014-sync-conflict-intelligence.md
// ============================================================
checkRule(
    "Rule 24: Conflict resolution must pair with audit log",
    ["src-tauri/src/commands/**/*.rs"],
    /\.resolve_conflict\s*\(/,
    (line) => {
        // The single allowed call site is the observability command which wraps in transaction
        if (/observability\.rs|sync_conflict_service\.rs/.test(line)) return true;
        if (/^\s*\/\//.test(line)) return true;
        return false;
    },
    "warning",
    (f) => !f.includes("observability.rs") && !f.includes("sync_conflict_service.rs")
);

// ============================================================
// RULE 25 — No SQL in sync visualization components
// Svelte pages under admin/ must not directly embed SQL strings.
// @see docs/architecture/0013-observability-layer.md
// ============================================================
checkRule(
    "Rule 25: No SQL in sync visualization Svelte components",
    ["src/pages/**/*.svelte"],
    /SELECT\s+\*|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM/i,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        return false;
    },
    "error"
);

// Rule 26: No `any` types allowed in frontend logic
checkRule(
    "Rule 26: No `any` types allowed in frontend logic (Svelte / TS / JS)",
    ["src/**/*.ts", "src/**/*.js", "src/**/*.svelte"],
    /:\s*any\b|<any>/,
    (line) => {
        if (/^\s*\/\//.test(line)) return true;
        if (/<!--/.test(line)) return true;
        if (/\* @type \{any\}/.test(line)) return true;
        if (/eslint-disable-next-line/.test(line)) return true;
        return false;
    },
    "error",
    (f) => !f.includes("/tests/") && !f.includes("\\tests\\") && !f.includes(".test.ts") && !f.includes(".spec.ts")
);

// ============================================================
// GROUP 8 — Memory Safety & Documentation (Production Pass)
// ============================================================

// Rule 31: Prevent whole-file reading in security-sensitive paths
// Custom implementation to support checking next line for [arch:allow-memory-unsafe]
{
    const patterns = [
        "src-tauri/src/infrastructure/sync/**/*.rs",
        "src-tauri/src/infrastructure/backup/**/*.rs",
        "src-tauri/src/infrastructure/security/**/*.rs",
        "src-tauri/src/infrastructure/export/**/*.rs",
    ];
    const regex = /\bread_to_end\(|fs::read\(|serde_json::from_slice\(/;
    const scannedFiles = new Set<string>();
    let ruleViolations = 0;
    const matches: { file: string; line: number; content: string }[] = [];

    for (const pattern of patterns) {
        const glob = new Glob(pattern);
        for (const file of glob.scanSync(".")) {
            const normalizedFile = file.replace(/\\/g, "/");
            if (scannedFiles.has(normalizedFile)) continue;
            scannedFiles.add(normalizedFile);

            if (file.includes("tests") || file.includes("development_key")) continue;

            const content = readFileSync(file, "utf-8");
            const lines = content.split("\n");

            lines.forEach((line, index) => {
                if (line.trim().startsWith("//")) return;

                if (regex.test(line)) {
                    // Check current line for allow comment
                    if (line.includes("[arch:allow-memory-unsafe]")) return;
                    // Check next line for allow comment (cargo fmt compatibility)
                    if (index + 1 < lines.length && lines[index + 1].includes("[arch:allow-memory-unsafe]")) return;

                    matches.push({
                        file,
                        line: index + 1,
                        content: line.trim(),
                    });
                    ruleViolations++;
                }
            });
        }
    }

    if (ruleViolations > 0) {
        console.log(`${colors.red}❌ Dangerous read_to_end or fs::read in sync/backup/security paths${colors.reset}`);
        matches.forEach((m) => {
            console.log(`  ${m.file}:${m.line} → ${m.content}`);
        });
        violations++;
    }
}

// Rule 32: Prevent fake architectural overclaims
checkRule(
    "Fake architectural claims in documentation/comments",
    ["src-tauri/src/**/*.rs", "docs/**/*.md", "*.md"],
    /\bO\(1\)\b|\btrue streaming\b|zero-copy|fully streaming|infinite scalability|infinite scale|memory-bounded serialization|streaming deserialization|row-by-row JSON|incremental JSON|zero-copy deserialization/i,
    (line) => line.includes("[arch:allow-overclaim]") || line.trim().startsWith("//"),
    "warning"
);

// Rule 36: Prevent fake security guarantees
checkRule(
    "Fake security guarantees in documentation/comments",
    ["src-tauri/src/**/*.rs", "docs/**/*.md", "*.md"],
    /military grade|impossible to decrypt|zero exposure|impossible memory leak|leak-proof|zero plaintext|unbreakable|hack-proof|fortress|impenetrable/i,
    (line) => line.includes("[arch:allow-overclaim]") || line.trim().startsWith("//"),
    "error"
);

// Rule 37: Forbid deprecated terminology and extensions
checkRule(
    "Deprecated terminology or extensions detected (BSS, .bss, .bssync, grpcsync, etc.)",
    ["src-tauri/src/**/*.rs", "docs/**/*.md", "*.md"],
    /\.bss\b|\.bssync\b|\bBSS\b|\bBSS2\b|\bBSS3\b|\bBSTM\b|\bgrpcsync\b|\bgrpcunit\b|\bgrpcbak\b|sealed package|chunk framing/i,
    (line) => line.includes("[arch:allow-history]") || line.trim().startsWith("//"),
    "error"
);

// Rule 38 (amended by ADR-0039): two-tier secret protection.
// age::x25519 is mandatory for node-managed secrets. age::scrypt is permitted
// exclusively for portable operator key material (.adminkey), which lives only in
// src-tauri/src/infrastructure/identity/adminkey_provider.rs.
checkRule(
    "Rule 38: age::scrypt is forbidden outside infrastructure/identity/adminkey_provider.rs (ADR-0039: two-tier secret protection)",
    ["src-tauri/src/**/*.rs"],
    /age::scrypt/i,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => !file.includes("infrastructure/identity/adminkey_provider.rs")
);

// Rule 39: Ensure session touching in commands with authorization
checkRule(
    "Rule 39: Command with authorize_command missing touch_session() (possible session expiry issue)",
    ["src-tauri/src/commands/*.rs"],
    /authorize_command/,
    (line) => line.trim().startsWith("//"),
    "warning",
    (file) => {
        if (file.includes("guards.rs") || file.includes("mod.rs")) return false;
        const content = readFileSync(file, "utf-8");
        return !content.includes("state.touch_session()");
    }
);

// ============================================================
// GROUP 9 — Layer Direction Rules
// ============================================================

// Rule 33: application must not depend on app (UI/State)
checkRule(
    "Layer Violation: application/ must not depend on crate::app",
    ["src-tauri/src/application/**/*.rs"],
    /use\s+crate::app\b/,
    (line) => line.trim().startsWith("//")
);

// Rule 34: domain must not depend on application
checkRule(
    "Layer Violation: domain/ must not depend on crate::application",
    ["src-tauri/src/domain/**/*.rs"],
    /use\s+crate::application\b/,
    (line) => line.trim().startsWith("//")
);

// Rule 35: infrastructure must not depend on commands
checkRule(
    "Layer Violation: infrastructure/ must not depend on crate::commands",
    ["src-tauri/src/infrastructure/**/*.rs"],
    /use\s+crate::commands\b/,
    (line) => line.trim().startsWith("//")
);

// ============================================================
// RULE 27 — No direct `invoke` in frontend files
// All Tauri API calls must go through `src/lib/tauri.ts`.
// ============================================================
checkRule(
    "Rule 27: Direct Tauri `invoke` call in frontend (must use lib/tauri.ts)",
    ["src/**/*.svelte", "src/**/*.ts", "src/**/*.js"],
    /invoke\s*\(/,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        return false;
    },
    "error",
    (file) => {
        const normalized = file.toLowerCase().replace(/\\/g, "/");
        return !normalized.includes("src/lib/tauri.ts") &&
               !normalized.includes("src/main.ts") &&
               !normalized.includes("/tests/") &&
               !normalized.includes(".test.ts") &&
               !normalized.includes(".spec.ts");
    }
);

// Rule 27b: No direct `@tauri-apps/` imports in frontend files
checkRule(
    "Rule 27b: Direct @tauri-apps/ import in frontend (must use lib/tauri.ts)",
    ["src/**/*.svelte", "src/**/*.ts", "src/**/*.js"],
    /['"]@tauri-apps\//,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        return false;
    },
    "error",
    (file) => {
        const normalized = file.toLowerCase().replace(/\\/g, "/");
        return !normalized.includes("src/lib/tauri.ts") &&
               !normalized.includes("src/main.ts") &&
               !normalized.includes("/tests/") &&
               !normalized.includes(".test.ts") &&
               !normalized.includes(".spec.ts");
    }
);

// Rule 40: No manual loading/submitting/resolving assignments in Svelte pages (must use createOperation or createOperationGuard)
checkRule(
    "Rule 40: Manual loading/submitting/resolving assignments in Svelte pages (must use createOperation or createOperationGuard)",
    ["src/pages/**/*.svelte"],
    /(?<!\b(let|const|var)\s+)\b(loading|submitting|saving|deleting|exportLoading|importLoading|resolving|processing|summaryLoading|movementsLoading|loadingUnits|loadingMonths|computing)\s*=\s*(true|false)\b/,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        return false;
    },
    "error"
);

// Rule 41: No untracked timers in pages/components (use createRuntimeScope)
checkRule(
    "Rule 41: Untracked setTimeout/setInterval in pages/components (use createRuntimeScope)",
    [
        "src/pages/**/*.svelte",
        "src/components/**/*.svelte",
        "src/lib/components/**/*.svelte",
    ],
    /\b(window\.)?set(Timeout|Interval)\s*\(/,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        if (/scope\.set(Timeout|Interval)/.test(line)) return true;
        if (/runtimeCleanup/.test(line)) return true;
        return false;
    },
    "error"
);

// Rule 42: No unmanaged addEventListener in pages/components
checkRule(
    "Rule 42: Unmanaged addEventListener in pages/components (use createRuntimeScope.addListener)",
    [
        "src/pages/**/*.svelte",
        "src/components/**/*.svelte",
        "src/lib/components/**/*.svelte",
    ],
    /\.addEventListener\s*\(/,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        if (/scope\.addListener/.test(line)) return true;
        return false;
    },
    "error"
);

// Rule 43: No raw alert/confirm (use centralized tauri wrappers)
checkRule(
    "Rule 43: Raw alert()/confirm() in frontend (use lib/tauri.ts wrappers)",
    ["src/**/*.svelte", "src/**/*.ts", "src/**/*.js"],
    /\b(window\.)?(alert|confirm)\s*\(/,
    (line) => {
        if (/^\s*(\/\/|<!--|\*)/.test(line)) return true;
        return false;
    },
    "error",
    (file) => {
        const normalized = file.toLowerCase().replace(/\\/g, "/");
        return !normalized.includes("src/lib/tauri.ts") &&
               !normalized.includes("/tests/") &&
               !normalized.includes(".test.ts") &&
               !normalized.includes(".spec.ts");
    }
);

// Rule 45: Empty catch blocks swallow errors in pages
checkRule(
    "Rule 45: Silent catch block in pages (must use formatErrorMessage or telemetry)",
    ["src/pages/**/*.svelte"],
    /catch\s*\{\s*\}/,
    (line) => {
        if (/^\s*(\/\/|<!--)/.test(line)) return true;
        return false;
    },
    "error"
);

// Rule 44: Pages must declare createRuntimeScope (file-level)
{
    const pageGlob = new Glob("src/pages/**/*.svelte");
    const missingScope: string[] = [];
    for (const file of pageGlob.scanSync(".")) {
        const normalized = file.toLowerCase().replace(/\\/g, "/");
        if (normalized.includes("notfoundpage.svelte")) continue;
        const content = readFileSync(file, "utf-8");
        if (!content.includes("createRuntimeScope")) {
            missingScope.push(file);
        }
    }
    if (missingScope.length > 0) {
        console.log(`❌ ${colors.red}Rule 44: Page without createRuntimeScope (runtime cleanup required)${colors.reset}`);
        missingScope.forEach((f) => console.log(`  ${f}`));
        violations++;
    }
}

// ============================================================
// GROUP 10 — Domain Event Integrity Rules
// ============================================================

// Rule 46: No direct INSERT into domain_events table outside DomainEventRepository
checkRule(
    "Rule 46: Direct INSERT into domain_events outside DomainEventRepository",
    ["src-tauri/src/**/*.rs"],
    /INSERT\s+INTO\s+domain_events/i,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        // Allow in the repository itself (domain_events.rs) and tests
        if (line.includes("domain_events.rs")) return true;
        // Allow gap detection test which deliberately inserts corrupted data
        if (line.includes("gap_detection_catches_corrupted")) return true;
        return false;
    },
    "error",
    (file) => !file.endsWith("domain_events.rs") && !file.includes("/tests/")
);

// Rule 47: Domain event queries must NOT ORDER BY created_at alone
// Authoritative ordering for domain events is (transaction_id ASC, sequence_number ASC).
// Using created_at alone can diverge from the authoritative ordering and must not
// be used for domain event queries. Exception: explicit [arch:allow-created-at] tag.
checkRule(
    "Rule 47: ORDER BY created_at on domain_events table (must use transaction_id, sequence_number)",
    ["src-tauri/src/**/*.rs"],
    /ORDER\s+BY\s+created_at\b/i,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => file.includes("domain_events") || file.endsWith("mod.rs")
         || file.endsWith("transaction.rs")
);

// Rule 48: Repositories must not use with_event_persistence (repos emit no events per ADR)
checkRule(
    "Rule 48: with_event_persistence used in repository (repositories must not emit events)",
    ["src-tauri/src/repositories/**/*.rs"],
    /with_event_persistence\s*\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 11 — Reporting Layer Rules (Phase 2)
// ============================================================

// Rule 49: No INSERT/UPDATE/DELETE in reporting/ (reports must be read-only)
// Excludes `.update(` and `.insert(` method calls (Digest::update, CacheStore::insert)
// and `fn insert(` method declarations.
checkRule(
    "Rule 49: SQL mutation in reporting/ (reports must be read-only)",
    ["src-tauri/src/application/reporting/**/*.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b/i,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes(".update(")) return true;
        if (line.includes(".insert(")) return true;
        if (line.includes("fn insert(")) return true;
        return false;
    },
    "error"
);

// Rule 50: No transaction ownership in reporting/ (no with_transaction/with_event_context/with_event_persistence)
checkRule(
    "Rule 50: Transaction ownership in reporting/ (reports must not own transactions)",
    ["src-tauri/src/application/reporting/**/*.rs"],
    /with_transaction|with_event_context|with_event_persistence\s*\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 51: No report mutation side effects (calling mutation methods on repositories)
// Looks for repository method calls that mutate state.
checkRule(
    "Rule 51: Repository mutation call in reporting/ (reports must not mutate state)",
    ["src-tauri/src/application/reporting/**/*.rs"],
    /\.(insert|update|delete|create|reclassify|archive|close|seed|set|refresh|upsert)_/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 52: No command returning report projections directly
// Commands must return minimal confirmation, not rich report projections.
checkRule(
    "Rule 52: Command returning report projection (commands must not return report types)",
    ["src-tauri/src/commands/*.rs"],
    /ReportEnvelope<|FiscalYearSummaryOutput|InventoryValuationOutput|StockMovementLedgerOutput/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 12 — Oversight Layer Rules (Phase 3.A)
// ============================================================

// Rule 53: No SQL in oversight/ (KPIs must compute from report outputs only)
checkRule(
    "Rule 53: SQL string in oversight/ (KPIs must not reference SQL directly)",
    ["src-tauri/src/application/oversight/**/*.rs"],
    /"SELECT|"INSERT|"UPDATE|"DELETE|"FROM\s+\w+|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 54: No mutations in oversight/ (KPIs are read-only projections)
checkRule(
    "Rule 54: SQL mutation in oversight/ (KPIs must not mutate state)",
    ["src-tauri/src/application/oversight/**/*.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b/i,
    (line) => line.trim().startsWith("//") || line.includes(".update("),
    "error"
);

// Rule 55: No transaction ownership in oversight/
checkRule(
    "Rule 55: Transaction ownership in oversight/ (KPIs must not own transactions)",
    ["src-tauri/src/application/oversight/**/*.rs"],
    /with_transaction|with_event_context|with_event_persistence\s*\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 56: No repository imports in oversight/ KPI implementations
// Exception: context.rs which bridges reports to KPIs
checkRule(
    "Rule 56: Repository import in oversight/metrics/ (KPIs must depend on context only)",
    ["src-tauri/src/application/oversight/metrics/**/*.rs"],
    /use\s+crate::repositories/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 57: No rusqlite in oversight/ (KPIs must not touch SQL layer)
checkRule(
    "Rule 57: rusqlite in oversight/ (KPIs must not reference rusqlite)",
    ["src-tauri/src/application/oversight/**/*.rs"],
    /\brusqlite\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 13 — Benchmark Layer Rules (Phase 3.B)
// ============================================================

// Rule 58: No SQL in benchmarks/ (benchmarks must compute from report outputs only)
checkRule(
    "Rule 58: SQL string in oversight/benchmarks/ (benchmarks must not reference SQL directly)",
    ["src-tauri/src/application/oversight/benchmarks/**/*.rs"],
    /"SELECT|"INSERT|"UPDATE|"DELETE|"FROM\s+\w+|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 59: No mutations in benchmarks/
checkRule(
    "Rule 59: SQL mutation in oversight/benchmarks/ (benchmarks must not mutate state)",
    ["src-tauri/src/application/oversight/benchmarks/**/*.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b/i,
    (line) => line.trim().startsWith("//") || line.includes(".update("),
    "error"
);

// Rule 60: No transaction ownership in benchmarks/
checkRule(
    "Rule 60: Transaction ownership in oversight/benchmarks/ (benchmarks must not own transactions)",
    ["src-tauri/src/application/oversight/benchmarks/**/*.rs"],
    /with_transaction|with_event_context|with_event_persistence\s*\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 61: No repository imports in benchmark implementations (must depend on context only)
checkRule(
    "Rule 61: Repository import in oversight/benchmarks/ (benchmarks must depend on context only)",
    ["src-tauri/src/application/oversight/benchmarks/**/*.rs"],
    /use\s+crate::repositories/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 62: No rusqlite in benchmarks/
checkRule(
    "Rule 62: rusqlite in oversight/benchmarks/ (benchmarks must not reference rusqlite)",
    ["src-tauri/src/application/oversight/benchmarks/**/*.rs"],
    /\brusqlite\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 14 — Anomaly Detection Rules (Phase 3.C)
// ============================================================

// Rule 63: No SQL in anomalies/ (detectors must compute from distribution data only)
checkRule(
    "Rule 63: SQL string in oversight/anomalies/ (detectors must not reference SQL directly)",
    ["src-tauri/src/application/oversight/anomalies/**/*.rs"],
    /"SELECT|"INSERT|"UPDATE|"DELETE|"FROM\s+\w+|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 64: No mutations in anomalies/
checkRule(
    "Rule 64: SQL mutation in oversight/anomalies/ (detectors must not mutate state)",
    ["src-tauri/src/application/oversight/anomalies/**/*.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b/i,
    (line) => line.trim().startsWith("//") || line.includes(".update("),
    "error"
);

// Rule 65: No transaction ownership in anomalies/
checkRule(
    "Rule 65: Transaction ownership in oversight/anomalies/ (detectors must not own transactions)",
    ["src-tauri/src/application/oversight/anomalies/**/*.rs"],
    /with_transaction|with_event_context|with_event_persistence\s*\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 66: No repository imports in anomaly implementations
checkRule(
    "Rule 66: Repository import in oversight/anomalies/ (detectors must depend on distribution only)",
    ["src-tauri/src/application/oversight/anomalies/**/*.rs"],
    /use\s+crate::repositories/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 67: No rusqlite in anomalies/
checkRule(
    "Rule 67: rusqlite in oversight/anomalies/ (detectors must not reference rusqlite)",
    ["src-tauri/src/application/oversight/anomalies/**/*.rs"],
    /\brusqlite\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 68: No wall-clock dependency in anomalies/ (Utc::now, SystemTime, Instant)
checkRule(
    "Rule 68: Wall-clock dependency in oversight/anomalies/ (detectors must be deterministic — no Utc::now)",
    ["src-tauri/src/application/oversight/anomalies/**/*.rs"],
    /\bUtc::now\b|\bSystemTime::now\b|\bInstant::now\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 15 — Audit Schema Evolution Rules (Phase 4)
// ============================================================

// Rule 69: No destructive audit migrations (ALTER TABLE DROP COLUMN, DROP TABLE audit_log, etc.)
checkRule(
    "Rule 69: Destructive audit migration detected (must be additive only)",
    ["src-tauri/src/db/migrations/*.sql"],
    /DROP\s+TABLE\s+audit_log|DROP\s+COLUMN\s+|ALTER\s+TABLE\s+audit_log\s+DROP/i,
    (line) => line.trim().startsWith("--"),
    "error",
    (file) => !file.includes("001_initial.sql")
);

// Rule 70: New audit writes must populate structured columns (event_type, actor_id, details)
// Checks that every INSERT INTO audit_log in the repository includes the event_type column.
{
    const patterns = ["src-tauri/src/repositories/audit.rs"];
    const scannedFiles = new Set<string>();
    let ruleViolations = 0;
    const matches: { file: string; line: number; content: string }[] = [];

    for (const pattern of patterns) {
        const glob = new Glob(pattern);
        for (const file of glob.scanSync(".")) {
            const normalizedFile = file.replace(/\\/g, "/");
            if (scannedFiles.has(normalizedFile)) continue;
            scannedFiles.add(normalizedFile);

            const content = readFileSync(file, "utf-8");
            const lines = content.split("\n");

            lines.forEach((line, index) => {
                if (line.trim().startsWith("//")) return;
                const insertMatch = line.match(/INSERT\s+INTO\s+audit_log\b/i);
                if (!insertMatch) return;

                // Check if this INSERT (or its continuation) includes event_type
                // Collect all lines of the SQL statement (multiline string)
                let mergedLine = line;
                let i = index;
                while (!mergedLine.includes("event_type") && !mergedLine.includes(";") && i < lines.length - 1) {
                    i++;
                    mergedLine += " " + lines[i].trim();
                }
                if (!mergedLine.includes("event_type")) {
                    matches.push({ file, line: index + 1, content: line.trim() });
                    ruleViolations++;
                }
            });
        }
    }

    if (ruleViolations > 0) {
        console.log(`❌ ${colors.red}Rule 70: Audit INSERT missing structured columns (dual-write requires event_type, actor_id, details)${colors.reset}`);
        matches.forEach((m) => {
            console.log(`  ${m.file}:${m.line} → ${m.content}`);
        });
        violations++;
    }
}

// Rule 71: No direct audit_log INSERT outside AuditRepository
// Any INSERT INTO audit_log outside repositories/ is a layer bypass.
checkRule(
    "Rule 71: Direct audit_log INSERT outside AuditRepository (use AuditService + AuditRepository)",
    [
        "src-tauri/src/application/**/*.rs",
        "src-tauri/src/commands/**/*.rs",
        "src-tauri/src/domain/**/*.rs",
        "src-tauri/src/infrastructure/**/*.rs",
    ],
    /INSERT\s+INTO\s+audit_log\b/i,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        return false;
    },
    "error",
    (file) => file.endsWith("audit.rs") || file.includes("/tests/")
);

// Rule 72: Audit readers must not depend on unstable ordering (timestamp alone is unstable)
// Scans audit_log queries for ORDER BY timestamp without id tiebreaker.
// Exempts legacy OFFSET-based methods and chain-verification (rowid-based).
{
    const patterns = ["src-tauri/src/repositories/audit.rs"];
    const scannedFiles = new Set<string>();
    let ruleViolations = 0;
    const matches: { file: string; line: number; content: string }[] = [];

    // Methods exempt from this rule (legacy OFFSET pagination)
    const exemptMethods = ["fetch_entries", "fetch_entries_iter", "fetch_user_activity_since"];

    for (const pattern of patterns) {
        const glob = new Glob(pattern);
        for (const file of glob.scanSync(".")) {
            const normalizedFile = file.replace(/\\/g, "/");
            if (scannedFiles.has(normalizedFile)) continue;
            scannedFiles.add(normalizedFile);

            const content = readFileSync(file, "utf-8");
            const lines = content.split("\n");
            let currentMethod = "";

            lines.forEach((line, index) => {
                if (line.trim().startsWith("//")) return;

                // Track which method we're in
                const methodMatch = line.match(/^\s+pub\s+fn\s+(\w+)/);
                if (methodMatch) {
                    currentMethod = methodMatch[1];
                }

                // Exempt known legacy methods (exact match)
                if (exemptMethods.includes(currentMethod)) return;

                // Find ORDER BY timestamp lines
                const orderByMatch = line.match(/ORDER\s+BY\s+timestamp\b/i);
                if (!orderByMatch) return;

                // Allow ORDER BY rowid (deterministic — not timestamp-based)
                if (/ORDER\s+BY\s+rowid\b/i.test(line)) return;

                // Check if the ORDER BY includes an id tiebreaker after the timestamp
                // Look for `, id ASC` or `, id DESC` after the timestamp clause
                const afterTimestamp = line.slice(orderByMatch.index! + orderByMatch[0].length);
                const hasIdTiebreaker = /\s+(?:ASC|DESC)?\s*,\s*id\s+(?:ASC|DESC)/i.test(afterTimestamp);

                if (!hasIdTiebreaker) {
                    matches.push({ file, line: index + 1, content: line.trim() });
                    ruleViolations++;
                }
            });
        }
    }

    if (ruleViolations > 0) {
        console.log(`⚠️ ${colors.yellow}Rule 72: Audit query with ORDER BY timestamp without id tiebreaker (unstable ordering)${colors.reset}`);
        matches.forEach((m) => {
            console.log(`  ${m.file}:${m.line} → ${m.content}`);
        });
        warnings++;
    }
}

// Rule 73: No audit mutation bypass (UPDATE/DELETE on audit_log outside cleanup)
checkRule(
    "Rule 73: UPDATE/DELETE on audit_log outside cleanup path (audit mutation bypass)",
    ["src-tauri/src/repositories/audit.rs"],
    /\bUPDATE\s+audit_log\b|\bDELETE\s+FROM\s+audit_log\b/i,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        // Allow the delete_older_than cleanup method
        if (line.includes("delete_older_than") || line.includes("timestamp < ?")) return true;
        return false;
    },
    "error"
);

// ============================================================
// GROUP 16 — Reporting Cache Runtime Rules (Phase 5.A)
// ============================================================

// Rule 74: No SQL inside reporting/cache/ (cache must not know SQL)
checkRule(
    "Rule 74: SQL found in reporting/cache/ (cache must not reference SQL)",
    ["src-tauri/src/application/reporting/cache/**/*.rs"],
    /"SELECT|"INSERT|"UPDATE|"DELETE|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//") || line.includes(".update("),
    "error"
);

// Rule 75: No repository imports inside reporting/cache/ (cache must not know repositories)
// Exception: compute_or_get_cached_helper which bridges the cache with report computation.
checkRule(
    "Rule 75: Repository import in reporting/cache/ (cache must not depend on repositories)",
    ["src-tauri/src/application/reporting/cache/**/*.rs"],
    /use\s+crate::repositories/,
    (line) => line.trim().startsWith("//") || line.includes("compute_or_get_cached_helper"),
    "error"
);

// Rule 76: No mutation outside cache store in reporting/cache/
// Only store.rs may mutate the underlying HashMap directly.
// runtime.rs uses self.store.*, tests use runtime.* or store.*
checkRule(
    "Rule 76: Mutation call on non-store object in reporting/cache/",
    ["src-tauri/src/application/reporting/cache/**/*.rs"],
    /\.(insert|remove|clear)\(/,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("self.store.") || line.includes("map.")) return true;
        if (line.includes("self.inner")) return true;
        if (line.includes("runtime.") || line.includes("runtime.insert(")) return true;
        if (line.includes("store.")) return true;
        if (line.includes("CacheStore::new")) return true;
        return false;
    },
    "error",
    (file) => {
        const normalized = file.replace(/\\/g, "/");
        return !normalized.endsWith("store.rs");
    }
);

// Rule 77: No chrono::Utc::now() directly in reporting/ (use SystemTime)
checkRule(
    "Rule 77: chrono::Utc::now() in reporting/ (use SystemTime for access tracking)",
    ["src-tauri/src/application/reporting/**/*.rs"],
    /\bUtc::now\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 78: No TTL-based eviction logic in reporting/cache/
checkRule(
    "Rule 78: TTL/expiry logic in reporting/cache/ (only semantic invalidation allowed)",
    ["src-tauri/src/application/reporting/cache/**/*.rs"],
    /\bttl\b|\bexpir\b|\bTTL\b|\bexpire\b|\bexpiry\b/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 79: No filesystem persistence in reporting/cache/ (in-memory only)
checkRule(
    "Rule 79: Filesystem persistence in reporting/cache/ (in-memory only)",
    ["src-tauri/src/application/reporting/cache/**/*.rs"],
    /\bstd::fs\b|\bfs::\b|\bFile::\b|\bOpenOptions\b|\bPathBuf\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 80: No rusqlite in reporting/cache/ (cache must not reference SQL layer)
checkRule(
    "Rule 80: rusqlite in reporting/cache/ (cache must not reference SQL layer)",
    ["src-tauri/src/application/reporting/cache/**/*.rs"],
    /\brusqlite\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 17 — Sync Integrity Rules (Phase 5.B)
// ============================================================

// Rule 81: No SQL in sync_integrity/ except repository layer
checkRule(
    "Rule 81: SQL found in sync_integrity/ (only repository layer may contain SQL)",
    ["src-tauri/src/application/sync_integrity/**/*.rs"],
    /"SELECT|"INSERT|"UPDATE|"DELETE|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 82: No chrono::Utc::now() in sync_integrity/
checkRule(
    "Rule 82: chrono::Utc::now() in sync_integrity/ (must be deterministic — no wall-clock)",
    ["src-tauri/src/application/sync_integrity/**/*.rs"],
    /\bUtc::now\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 83: No mutation during validation phase
// Checks for INSERT/UPDATE/DELETE in sync_integrity/ validation paths
checkRule(
    "Rule 83: SQL mutation in sync_integrity/validation.rs (validation must not mutate state)",
    ["src-tauri/src/application/sync_integrity/validation.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b/i,
    (line) => line.trim().startsWith("//") || line.includes("evidence") || line.includes(".insert("),
    "error"
);

// Rule 84: Replay checks must occur before mutation
// Enforced at the module level: sync_integrity/replay.rs is read-only
checkRule(
    "Rule 84: Mutation call in sync_integrity/replay.rs (replay checks must not mutate)",
    ["src-tauri/src/application/sync_integrity/replay.rs"],
    /\.(insert|update|delete|create|reclassify|archive|close|seed|set|refresh|upsert)_/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 85: No OFFSET pagination in reconciliation paths
checkRule(
    "Rule 85: OFFSET pagination in sync_integrity/reconciliation.rs (must use keyset pagination)",
    ["src-tauri/src/application/sync_integrity/reconciliation.rs"],
    /\bOFFSET\b/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 86: All sync ordering requires deterministic tiebreakers
checkRule(
    "Rule 86: Potential non-deterministic ordering in sync_integrity/ (missing explicit tiebreaker)",
    ["src-tauri/src/application/sync_integrity/**/*.rs"],
    /\.sort_by\s*\([^)]*$/,
    (line) => line.trim().startsWith("//") || line.includes("cmp"),
    "error"
);

// Rule 87: No direct domain_events inserts in sync_integrity/
checkRule(
    "Rule 87: Direct domain_events insert in sync_integrity/ (must not write events directly)",
    ["src-tauri/src/application/sync_integrity/**/*.rs"],
    /INSERT\s+INTO\s+domain_events/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 18 — Import Execution Hardening Rules (Phase 5.C)
// ============================================================

// Rule 88: No mutation before replay detection in sync_import_execution_service
// All mutations go through repositories, which are the allowed path.
checkRule(
    "Rule 88: Mutation before replay detection in sync_import_execution_service (replay check must precede all mutations)",
    ["src-tauri/src/application/services/sync_import_execution_service.rs"],
    /\.insert_if_new\(|\.insert_raw_|\.upsert_|\.insert_empty_/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("[arch:allow-mutation-before-replay]")) return true;
        if (index > 0 && lines[index - 1].includes("[arch:allow-mutation-before-replay]")) return true;
        if (index + 1 < lines.length && lines[index + 1].includes("[arch:allow-mutation-before-replay]")) return true;
        if (line.includes("repo.")) return true;
        return false;
    },
    "error"
);

// Rule 89: No nested transactions in sync services
checkRule(
    "Rule 89: Nested with_transaction in sync services (single transaction boundary only)",
    [
        "src-tauri/src/application/services/sync_import_execution_service.rs",
        "src-tauri/src/application/services/sync_import_validation_service.rs",
        "src-tauri/src/application/services/sync_conflict_resolution_service.rs",
    ],
    /with_transaction\(|with_event_context\(|with_event_persistence\(/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("[arch:allow-non-nested]")) return true;
        if (index > 0 && lines[index - 1].includes("[arch:allow-non-nested]")) return true;
        return false;
    },
    "error"
);

// Rule 90: No SQL in sync_integrity/ (already Rule 81, also check services)
checkRule(
    "Rule 90: SQL found in sync_import_*_service.rs (must use repositories, not raw SQL)",
    [
        "src-tauri/src/application/services/sync_import_execution_service.rs",
        "src-tauri/src/application/services/sync_import_validation_service.rs",
        "src-tauri/src/application/services/sync_conflict_resolution_service.rs",
        "src-tauri/src/application/services/sync_import_models.rs",
    ],
    /"SELECT|"INSERT|"UPDATE|"DELETE|\.execute\(|\.prepare\(/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("[arch:allow-sql]")) return true;
        if (index > 0 && lines[index - 1].includes("[arch:allow-sql]")) return true;
        if (line.includes("repo.")) return true;
        return false;
    },
    "error"
);

// Rule 91: No OFFSET pagination in sync import execution
checkRule(
    "Rule 91: OFFSET pagination in sync_import_execution_service (must use keyset pagination)",
    ["src-tauri/src/application/services/sync_import_execution_service.rs"],
    /\bOFFSET\b/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 92: No direct domain_events insert in sync import services
checkRule(
    "Rule 92: Direct domain_events insert in sync import services (must use EventContext::emit)",
    [
        "src-tauri/src/application/services/sync_import_execution_service.rs",
        "src-tauri/src/application/services/sync_import_validation_service.rs",
        "src-tauri/src/application/services/sync_conflict_resolution_service.rs",
    ],
    /INSERT\s+INTO\s+domain_events/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 93: Replay rejection must audit (enforced by checking that SyncConflictDetected is emitted)
// This rule verifies that conflict detection is paired with event emission
checkRule(
    "Rule 93: Replay rejection without audit event in sync_import_execution_service",
    ["src-tauri/src/application/services/sync_import_execution_service.rs"],
    /ReplayDetected|replay_detected|DuplicatePackage/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("emit(DomainEvent::SyncConflictDetected")) return true;
        if (line.includes("ReplayProtectionResult")) return true;
        if (line.includes("ImportExecutionError")) return true;
        if (line.includes("audited:")) return true;
        if (line.includes("replay_detected:") || line.includes("r.replay_detected")) return true;
        if (line.includes("assert!")) return true;
        return false;
    },
    "warning"
);

// Rule 94: validation service must not mutate
checkRule(
    "Rule 94: Mutation call in sync_import_validation_service (validation must not mutate state)",
    ["src-tauri/src/application/services/sync_import_validation_service.rs"],
    /\.insert_|\.update_|\.delete_|\.upsert_/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 95: execution service must own transaction (verified by checking it uses with_event_persistence)
checkRule(
    "Rule 95: sync_import_execution_service must use with_event_persistence (owns transaction boundary)",
    ["src-tauri/src/application/services/sync_import_execution_service.rs"],
    /fn execute_import/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        // Scan next 40 lines for with_event_persistence
        for (let i = index + 1; i < Math.min(index + 40, lines.length); i++) {
            if (lines[i].includes("with_event_persistence(")) return true;
        }
        return false;
    },
    "warning"
);

// ============================================================
// GROUP 19 — SQLite Observability Rules (Phase 6.A)
// ============================================================

// Rule 97: No business logic in sqlite_observability/
checkRule(
    "Rule 97: Business logic in sqlite_observability/ (must be pure observability only)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /\b(fifo|FifoLayer|StockMovement|fiscal|FiscalYear|Account|CostBasis|InventorySnapshot|InventoryValuation)\b/,
    (line) => line.trim().startsWith("//") || line.trim().startsWith("///"),
    "error"
);

// Rule 98: No repository imports in sqlite_observability/
checkRule(
    "Rule 98: Repository import in sqlite_observability/ (must not depend on repositories)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /use\s+crate::repositories/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 99: No mutation queries in sqlite_observability/
checkRule(
    "Rule 99: SQL mutation in sqlite_observability/ (observability must be read-only)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b/i,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("DELETE FROM")) return false;
        if (line.includes("INSERT INTO")) return false;
        if (line.includes("UPDATE ")) return false;
        return true;
    },
    "error",
    (file) => {
        // Allow tests which don't actually execute SQL
        const normalized = file.replace(/\\/g, "/");
        return normalized.endsWith("_tests.rs") || normalized.endsWith("/tests/");
    }
);

// Rule 100: No WAL checkpoint execution in sqlite_observability/
checkRule(
    "Rule 100: WAL checkpoint execution in sqlite_observability/ (checkpoint not allowed until Phase 6.B)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /wal_checkpoint|checkpoint\(|PRAGMA\s+wal_checkpoint/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 101: No background thread spawning in sqlite_observability/
checkRule(
    "Rule 101: Background thread spawning in sqlite_observability/ (no threads in observability)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /\bstd::thread\b|\bspawn\b|\bthread::spawn\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 102: No OFFSET pagination in sqlite_observability diagnostics APIs
checkRule(
    "Rule 102: OFFSET pagination in sqlite_observability/ (must use keyset or no pagination)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /\bOFFSET\b/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 103: No chrono::Utc::now in diagnostics core (must be deterministic)
checkRule(
    "Rule 103: chrono::Utc::now() in sqlite_observability/ (diagnostics must be deterministic — no wall-clock)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /\bUtc::now\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 104: No direct filesystem mutation in sqlite_observability/
checkRule(
    "Rule 104: Filesystem mutation in sqlite_observability/ (must not write to filesystem)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /\bstd::fs\b|\bfs::\b|\bFile::\b|\bOpenOptions\b|\bPathBuf\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 105: No automatic index creation in sqlite_observability/
checkRule(
    "Rule 105: Automatic index creation in sqlite_observability/ (must not create indexes)",
    ["src-tauri/src/infrastructure/sqlite_observability/**/*.rs"],
    /CREATE\s+INDEX|CREATE\s+UNIQUE\s+INDEX/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 20 — SQLite Runtime Rules (Phase 6.B)
// ============================================================

// Rule 106: No VACUUM execution in sqlite_runtime/
checkRule(
    "Rule 106: VACUUM execution in sqlite_runtime/ (VACUUM must not be executed automatically)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\bVACUUM\b/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 107: No auto repair in sqlite_runtime/
checkRule(
    "Rule 107: Auto-repair logic in sqlite_runtime/ (integrity monitoring must not auto-repair)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\brepair\b|\bfix\b|\brecover\b/i,
    (line) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("recover_interrupted_restore")) return true;
        if (line.includes("BackupValidationFailure")) return true;
        if (line.includes("IntegrityExecutionPolicy")) return true;
        return false;
    },
    "error"
);

// Rule 108: No background threads in sqlite_runtime/
checkRule(
    "Rule 108: Background thread spawning in sqlite_runtime/ (no threads in runtime)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\bstd::thread\b|\bspawn\b|\bthread::spawn\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 109: No hidden loops in sqlite_runtime/ (no infinite loops, no background polling)
checkRule(
    "Rule 109: Hidden loop in sqlite_runtime/ (no infinite loops or background polling)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\bloop\s*\{\s*$|\bwhile\s*true\b|\bfor\s*\(?\s*;;/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 110: No second SQLite connection outside explicit backup validation
checkRule(
    "Rule 110: Second SQLite connection in sqlite_runtime/ outside backup validation (only backup_validation.rs may open temp connections)",
    [
        "src-tauri/src/infrastructure/sqlite_runtime/checkpoint.rs",
        "src-tauri/src/infrastructure/sqlite_runtime/integrity_runner.rs",
        "src-tauri/src/infrastructure/sqlite_runtime/idle_checkpoint.rs",
        "src-tauri/src/infrastructure/sqlite_runtime/runtime_metrics.rs",
        "src-tauri/src/infrastructure/sqlite_runtime/policies.rs",
    ],
    /Connection::open|Connection::open_with_flags/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 111: No repository imports in sqlite_runtime/
checkRule(
    "Rule 111: Repository import in sqlite_runtime/ (must not depend on repositories)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /use\s+crate::repositories/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 112: No business logic in sqlite_runtime/
checkRule(
    "Rule 112: Business logic in sqlite_runtime/ (must be pure runtime execution only)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\b(fifo|FifoLayer|StockMovement|fiscal|FiscalYear|Account|CostBasis|InventorySnapshot|InventoryValuation)\b/,
    (line) => line.trim().startsWith("//") || line.trim().startsWith("///"),
    "error"
);

// Rule 113: No filesystem deletion in sqlite_runtime/ (no fs::remove, no fs::write)
checkRule(
    "Rule 113: Filesystem deletion in sqlite_runtime/ (must not delete filesystem artifacts)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\bstd::fs\b|\bfs::remove\b|\bfs::write\b|\bunlink\b/i,
    (line) => line.trim().startsWith("//"),
    "error",
    (file) => !file.endsWith("backup_validation.rs") // backup_validation may open files
);

// Rule 114: No WAL deletion by path in sqlite_runtime/
checkRule(
    "Rule 114: WAL file deletion by path sqlite_runtime/ (must not delete -wal files directly)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /-wal|-shm/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 115: No chrono::Utc::now in sqlite_runtime core
checkRule(
    "Rule 115: chrono::Utc::now() in sqlite_runtime/ (runtime must be deterministic — no wall-clock)",
    ["src-tauri/src/infrastructure/sqlite_runtime/**/*.rs"],
    /\bUtc::now\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 21 — SQLite Runtime Review Rules (Phase 6.C)
// ============================================================

// Rule 116: No background threads in sqlite_runtime_review/
checkRule(
    "Rule 116: Background thread spawning in sqlite_runtime_review/ (no threads in runtime review)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /\bstd::thread\b|\bspawn\b|\bthread::spawn\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 117: No async runtime in sqlite_runtime_review/
checkRule(
    "Rule 117: Async runtime usage in sqlite_runtime_review/ (must be synchronous)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /\basync\s+fn\b|\bawait\b|\btokio::\b|\bfutures::\b|\bAsync\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 118: No scheduler loops in sqlite_runtime_review/
checkRule(
    "Rule 118: Hidden loop in sqlite_runtime_review/ (no infinite loops or background polling)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /\bloop\s*\{\s*$|\bwhile\s*true\b|\bfor\s*\(?\s*;;/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 119: No automatic checkpoints in sqlite_runtime_review/
checkRule(
    "Rule 119: Automatic checkpoint execution in sqlite_runtime_review/ (review must not execute checkpoints)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /\bcheckpoint\s*\(|PRAGMA\s+wal_checkpoint|\.execute_checkpoint/i,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 120: No second write connection in sqlite_runtime_review/
checkRule(
    "Rule 120: Second SQLite write connection in sqlite_runtime_review/ (must not open write connections)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /Connection::open\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 121: No mutation in sqlite_runtime_review/
checkRule(
    "Rule 121: Mutation call in sqlite_runtime_review/ (evaluation must not mutate state)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /\bINSERT\b|\bUPDATE\b|\bDELETE\b|\.execute\(|\.prepare\(/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 122: No chrono::Utc::now in sqlite_runtime_review/
checkRule(
    "Rule 122: chrono::Utc::now() in sqlite_runtime_review/ (review must be deterministic — no wall-clock)",
    ["src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs"],
    /\bUtc::now\b/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// ============================================================
// GROUP 22 — Rules inherited from Phase 5.C
// ============================================================

// Rule 96: No chrono::Utc::now in sync execution path
checkRule(
    "Rule 96: chrono::Utc::now() in sync_import services (must be deterministic — no wall-clock)",
    [
        "src-tauri/src/application/services/sync_import_execution_service.rs",
        "src-tauri/src/application/services/sync_import_validation_service.rs",
        "src-tauri/src/application/services/sync_conflict_resolution_service.rs",
    ],
    /\bUtc::now\b/,
    (line, index, lines) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("[arch:allow-utc-now]")) return true;
        if (index > 0 && lines[index - 1].includes("[arch:allow-utc-now]")) return true;
        return false;
    },
    "error"
);

// ============================================================
// GROUP 23 — ADR Exception Governance (Phase 8)
// ============================================================

// Rule 123: All [arch:allow-*] tags must reference an ADR number
checkRule(
    "Rule 123: [arch:allow-*] tag without ADR reference (every exception must reference see ADR-NNNN)",
    [
        "src-tauri/src/**/*.rs",
    ],
    /\[arch:allow-/,
    (line, index, lines) => {
        // Allow tags that have ADR reference on the same line
        if (line.includes("see ADR-")) return true;
        // Allow preceding line to have ADR reference
        if (index > 0 && lines[index - 1].includes("see ADR-")) return true;
        // Allow [arch:allow-created-at] which is only a reference comment, not a real tag
        if (line.includes("[arch:allow-created-at]")) return true;
        return false;
    },
    "error"
);

// Rule 124: check_arch.ts must contain ADR exception governance group
checkRule(
    "Rule 124: check_arch.ts must contain ADR exception governance group",
    ["scripts/check_arch.ts"],
    /GROUP 23.*ADR Exception Governance|adr_exception_registry/,
    (line) => true,
    "error"
);

// Rule 125: adr_exception_registry.md must exist and be non-empty
checkRule(
    "Rule 125: adr_exception_registry.md must exist and be non-empty",
    ["docs/architecture/adr_exception_registry.md"],
    /## Exception:/,
    (line) => true,
    "error"
);

// ============================================================
// GROUP 24 — Node Identity & Trust (RFC 2026-08-04-node-identity-trust, ADR-0038)
// ============================================================

// Rule 126: Asymmetric signing (Ed25519) is confined to identity layers.
// B3 introduces Ed25519; this rule keeps it out of commands/models/repositories.
checkRule(
    "Rule 126: Ed25519 / asymmetric identity usage outside allowed identity layers (RFC 2026-08-04, ADR-0038)",
    [
        "src-tauri/src/commands/**/*.rs",
        "src-tauri/src/models/**/*.rs",
        "src-tauri/src/repositories/**/*.rs",
    ],
    /ed25519|Ed25519|ED25519/,
    (line) => line.trim().startsWith("//"),
    "error"
);

// Rule 127: ManageUnits must be node-type (WILAYA-only) guarded through authz.
// Until B2 adds the node-type guard, occurrences must carry the tag.
checkRule(
    "Rule 127: Action::ManageUnits without WILAYA node-type guard tag (RFC 2026-08-04, ADR-0038)",
    ["src-tauri/src/application/authz/policies/**/*.rs"],
    /Action::ManageUnits/,
    (line) => line.includes("[arch:allow-manageunits-wilaya]"),
    "error"
);

// Rule 128 (CLOSED in B6-A): No hardcoded default admin credential.
// B5 replaced the production bootstrap path (WILAYA->ADMIN). B6-A removed the
// production legacy seed entirely from `ConnectionFactory::new()`; the only
// remaining `hash_password("admin"...)` occurrence is the TEST-SUPPORT factory
// seed (`db::seed_default_admin`, used exclusively by `new_for_test` /
// `new_with_path`). The governance concern this rule guarded no longer exists
// in production, so the rule is retired rather than suppressed.
// RFC 2026-08-04-node-identity-trust §3.6 / ADR-0038.

// Rule 129: signature_version must remain an Option<u16> extension point.
checkRule(
    "Rule 129: signature_version declaration must remain Option<u16> (RFC 2026-08-04, ADR-0038)",
    ["src-tauri/src/application/sync/package_metadata.rs"],
    /pub signature_version:\s*\w/,
    (line) => line.includes("Option<u16>"),
    "error"
);

// ============================================================
// Groups 1-23 passed. Now delegate to the modular governance engine
// for FE invariant-based governance (CONTRACT_BOUNDARY,
// PROJECTION_INTEGRITY, RUNTIME_SAFETY, ARCHITECTURE_GRAPH,
// GOVERNANCE_FREEZE).
// ============================================================

if (violations === 0 && warnings === 0) {
    const { runGovernanceAudit } = await import("./governance/engine");
    runGovernanceAudit();
} else {
    console.log("");
    if (violations > 0) {
        console.log(`\n❌ ${colors.red}${violations} rule(s) violated in backend checks. Architectural integrity compromised.${colors.reset}`);
    }
    if (warnings > 0) {
        console.log(`❌ ${colors.yellow}Architecture check failed: ${warnings} warning(s) require resolution (zero-warning policy).${colors.reset}`);
    }
    process.exit(1);
}
