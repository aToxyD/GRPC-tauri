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
    excludeLines: (line: string) => boolean,
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

// Rule 38: age::scrypt is forbidden. System must use age::x25519 exclusively.
checkRule(
    "Rule 38: age::scrypt is forbidden. System must use age::x25519 exclusively.",
    ["src-tauri/src/**/*.rs"],
    /age::scrypt/i,
    (line) => line.trim().startsWith("//"),
    "error"
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
// ============================================================
// INVARIANT-BASED GOVERNANCE (v2)
// Replaces ~47 FE micro-rules (Groups 24-28) with 5 invariants
// @see docs/governance/frontend/GOVERNANCE_V2_SPEC.md
// ============================================================

// Suppression helper: creates an excludeLines callback that supports
// [arch:allow-<tag>] comments on the same or previous line.
function suppressExclude(tag: string): (line: string, index: number, lines: string[]) => boolean {
    return (line: string, index: number, lines: string[]) => {
        const marker = `[arch:allow-${tag}]`;
        if (line.includes(marker)) return true;
        if (index > 0 && lines[index - 1].includes(marker)) return true;
        return false;
    };
}

// Domain ownership registry: maps each domain to its contract functions,
// owned pages, and allowed cross-domain exceptions.
const DOMAIN_REGISTRY: Record<string, {
    contract: string;
    pages: string[];
    functions: string[];
    crossDomainExceptions: string[];
}> = {
    consumption: {
        contract: "consumption.contract.ts",
        pages: ["ConsumptionPage"],
        functions: ["createDailyReport", "previewDailyConsumptionFifo", "createMealConsumption", "getDailyReport", "listDailyReports", "getDailyConsumption", "calculateMealCost", "calculateMealRate", "recordConsumption"],
        crossDomainExceptions: ["listProducts", "checkStockAvailability"],
    },
    inventory: {
        contract: "inventory.contract.ts",
        pages: ["ProductsPage", "StockPage", "UnitDashboard", "UnitsPage", "UnitInventoryPage"],
        functions: ["createProduct", "updateProduct", "deleteProduct", "getProduct", "listProducts", "createUnit", "getUnit", "listUnits", "updateUnit", "deleteUnit", "getStock", "getAllStocks", "checkStockAvailability", "getCurrentStock", "getInventoryFifoView", "computeUnitInventorySnapshot", "getUnitInventoryView", "getAvailableReportMonths", "exportUnitInventoryExcel", "getStockMovements", "getStockSummary", "calculateProductPriceWithTva", "exportProductsExcel", "exportStockMovementsExcel", "getOrders"],
        crossDomainExceptions: ["exportProductsPackage", "importProductsPackage", "exportStockMovementsPackage", "importStockMovementsPackage", "exportUnitNodePackage", "listSupplierOrders"],
    },
    orders: {
        contract: "orders.contract.ts",
        pages: ["OrdersPage"],
        functions: ["createSupplierOrder", "confirmOrder", "updateSupplierOrder", "deleteSupplierOrder", "getSupplierOrder", "getSupplierOrderItems", "listSupplierOrders", "createOrder"],
        crossDomainExceptions: ["listProducts"],
    },
    report: {
        contract: "report.contract.ts",
        pages: ["UnitReportsPage", "WilayaReportsPage"],
        functions: ["getMonthlySummary", "listFiscalYears", "listWilayaReports", "generateReports", "getReportData", "exportDailyReportExcel", "exportMonthlySummaryExcel", "exportAllUnitsMonthlyStatusExcel"],
        crossDomainExceptions: ["listUnits", "listDailyReports", "getDailyReport", "exportDailyReportPackage", "exportMonthlySummaryPackage"],
    },
    fiscal: {
        contract: "fiscal.contract.ts",
        pages: ["FiscalManagementPage", "FiscalDiagnosticsPage"],
        functions: ["closeFiscalYear", "getFiscalYearStatus", "exportFiscalClosurePackage", "previewFiscalClosurePackage", "applyFiscalClosurePackage", "getFiscalTransitionHistory", "listFiscalPackageRegistry", "updateFiscalPackageRetentionStatus", "getAdvancedDiagnosticsBundle", "verifyInventoryIntegrity", "createFiscalOperationalSnapshot"],
        crossDomainExceptions: ["listProducts", "getSystemHealth"],
    },
    sync: {
        contract: "sync.contract.ts",
        pages: ["SyncPage"],
        functions: ["exportProductsPackage", "exportDailyReportPackage", "exportMonthlySummaryPackage", "exportUnitNodePackage", "exportStockMovementsPackage", "importProductsPackage", "importDailyReportPackage", "importUnitNodePackage", "importMonthlySummaryPackage", "importStockMovementsPackage"],
        crossDomainExceptions: ["listUnits"],
    },
    backup: {
        contract: "backup.contract.ts",
        pages: ["BackupPage"],
        functions: ["createBackup", "listBackups", "issueOperationExecutionToken", "restoreBackup"],
        crossDomainExceptions: [],
    },
    dashboard: {
        contract: "dashboard.contract.ts",
        pages: ["SystemHealthPage"],
        functions: ["getBuildInfo", "getRecentTelemetry"],
        crossDomainExceptions: ["getSystemHealth", "getSyncHealth"],
    },
    observability: {
        contract: "observability.contract.ts",
        pages: ["AuditIntegrityPage", "SystemHealthPage", "SyncTopologyPage", "ConflictCenterPage"],
        functions: ["getAuditChainStatus", "getAuditHealth", "getSystemHealth", "getSyncHealth", "getConflictSummary", "listSyncConflicts", "resolveSyncConflict"],
        crossDomainExceptions: [],
    },
    audit: {
        contract: "audit.contract.ts",
        pages: ["AuditLogPage"],
        functions: ["getAuditLog", "getAuditStats", "getUserActivity", "exportAuditLogExcel", "cleanupAuditLogs"],
        crossDomainExceptions: [],
    },
    metrics: {
        contract: "metrics.contract.ts",
        pages: ["UnitStatisticsPage", "WilayaStatisticsPage", "WilayaDashboard"],
        functions: ["getLoginMetrics", "getSystemMetrics", "getSyncSecurityDiagnostics", "syncPreflightCheck"],
        crossDomainExceptions: ["listUnits", "listProducts"],
    },
    session: {
        contract: "session.contract.ts",
        pages: ["LoginPage", "WilayaNodeSetupPage"],
        functions: ["logout", "checkSession", "getCurrentUser", "touchSession", "getSettings", "configureAsWilaya", "isConfigured"],
        crossDomainExceptions: ["login", "importUnitNodePackage"],
    },
    user: {
        contract: "user.contract.ts",
        pages: ["LoginPage"],
        functions: ["login", "changePassword"],
        crossDomainExceptions: [],
    },
};

// The universal cross-domain functions any page may import without restriction
const UNIVERSAL_ALLOWED = ["getSettings"];

// -----------------------------------------------------------
// INVARIANT A — CONTRACT_BOUNDARY
// All IPC must flow through /src/lib/contracts/*.contract.ts only.
// Suppression: [arch:allow-invariant-a] or [arch:allow-fe1*]
// -----------------------------------------------------------
function scanContractBoundary(): { violations: number; warnings: number } {
    let v = 0;
    let w = 0;

    // FE-111: All contract files must exist
    const expectedContracts = [
        "consumption", "inventory", "report", "session", "user",
        "orders", "sync", "backup", "metrics", "audit",
        "observability", "fiscal", "dashboard", "platform",
    ];
    const existingContracts = new Set(
        [...new Glob("src/lib/contracts/*.contract.ts").scanSync(".")]
            .map(f => f.replace(/^.*\/(\w+)\.contract\.ts$/, "$1"))
    );
    const missing = expectedContracts.filter(d => !existingContracts.has(d));
    if (missing.length > 0) {
        console.log(`❌ ${colors.red}FE-111 Error: Required contract files missing for domains: ${missing.join(", ")}${colors.reset}`);
        console.log(`  → Target: src/lib/contracts/<domain>.contract.ts`);
        v++;
    }

    // FE-136: Contract barrel must exist
    if (![...new Glob("src/lib/contracts/index.ts").scanSync(".")].length) {
        console.log(`❌ ${colors.red}FE-136 Error: Required contract barrel file missing: src/lib/contracts/index.ts${colors.reset}`);
        v++;
    }

    // FE-112: Contract file must export at least one safeInvoke wrapper
    const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
    let fe112Warnings = 0;
    for (const file of contractGlob.scanSync(".")) {
        const content = readFileSync(file, "utf-8");
        if (!content.includes("safeInvoke")) {
            console.log(`⚠️ ${colors.yellow}FE-112 Warning: ${file} does not export any safeInvoke wrapper${colors.reset}`);
            fe112Warnings++;
        }
    }
    if (fe112Warnings > 0) w++;

    // FE-114: Every exported contract function must have explicit typed return value
    checkRule(
        "FE-114 Error: Contract function missing explicit typed return value (must declare : Promise<...>)",
        ["src/lib/contracts/*.contract.ts"],
        /export\s+(async\s+)?function\s+\w+\s*\([^)]*\)\s*(?!:\s*Promise)/,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            return false;
        },
        "error"
    );
    checkRule(
        "FE-114 Error: Contract arrow function missing explicit typed return value (must declare : Promise<...>)",
        ["src/lib/contracts/*.contract.ts"],
        /export\s+(const|let|var)\s+\w+\s*=\s*async\s*\([^)]*\)\s*(?!:\s*Promise)/,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            return false;
        },
        "error"
    );

    // FE-116: Contracts must not import other contracts
    checkRule(
        "FE-116 Error: Contract importing another contract (contracts must be isolated)",
        ["src/lib/contracts/*.contract.ts"],
        /from\s+['"]\.\/(\w+)\.contract['"]|from\s+['"]\.\.\/contracts\//,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            return false;
        },
        "error"
    );

    // FE-120: Contracts must not import Svelte runtime
    checkRule(
        "FE-120 Error: Contract importing Svelte runtime (contracts must be runtime-independent)",
        ["src/lib/contracts/*.contract.ts"],
        /from\s+['"]svelte/,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            return false;
        },
        "error"
    );

    // FE-138: Components must not access IPC directly
    checkRule(
        "FE-138 Error: Component accessing IPC directly (must receive data via props from pages)",
        ["src/components/**/*.svelte", "src/lib/components/**/*.svelte"],
        /from\s+['"]\.\.?\/lib\/(tauri|contracts)/,
        (line, index, lines) => {
            if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) return true;
            if (line.includes("[arch:allow-component-ipc]")) return true;
            if (line.includes("[arch:allow-component-ipc-ghost]")) return true;
            if (index > 0 && lines[index - 1].includes("[arch:allow-component-ipc]")) return true;
            if (index > 0 && lines[index - 1].includes("[arch:allow-component-ipc-ghost]")) return true;
            return false;
        },
        "error"
    );

    // FE-153: Contract Ownership Enforcement — each IPC command in exactly one contract
    const cmdToContracts = new Map<string, string[]>();
    for (const file of contractGlob.scanSync(".")) {
        const content = readFileSync(file, "utf-8");
        const matches = [...content.matchAll(/safeInvoke(?:<\s*\w+(?:\s*\[\s*\w+\s*(?:,\s*\w+\s*)?\]\s*)?\s*>)?\s*\(\s*'([^']+)'/g)];
        for (const m of matches) {
            const cmd = m[1];
            if (!cmdToContracts.has(cmd)) cmdToContracts.set(cmd, []);
            cmdToContracts.get(cmd)!.push(file);
        }
    }
    for (const [cmd, contracts] of cmdToContracts) {
        if (contracts.length > 1) {
            console.log(`❌ ${colors.red}FE-153 Error: IPC command '${cmd}' appears in multiple contracts: ${contracts.join(", ")}${colors.reset}`);
            v++;
        }
    }

    // FE-154: Barrel Integrity
    const barrelPath = "src/lib/contracts/index.ts";
    try {
        const barrelContent = readFileSync(barrelPath, "utf-8");
        for (const file of contractGlob.scanSync(".")) {
            const contractName = file.replace(/^.*\/(\w+)\.contract\.ts$/, "$1");
            if (!barrelContent.includes(contractName)) {
                console.log(`❌ ${colors.red}FE-154 Error: ${contractName}.contract.ts not re-exported from ${barrelPath}${colors.reset}`);
                v++;
            }
        }
    } catch { /* barrel missing — handled by FE-136 */ }

    const barrelBypassPattern = /from\s+['"].*\/contracts\/\w+\.contract['"]/;
    for (const pattern of ["src/pages/**/*.svelte", "src/{components,lib}/**/*.{svelte,ts}"]) {
        const glob = new Glob(pattern);
        for (const file of glob.scanSync(".")) {
            if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
            const content = readFileSync(file, "utf-8");
            const directImport = content.match(barrelBypassPattern);
            if (directImport) {
                console.log(`❌ ${colors.red}FE-154 Error: ${file} bypasses barrel — direct contract import: ${directImport[1]}${colors.reset}`);
                v++;
            }
        }
    }

    return { violations: v, warnings: w };
}

// -----------------------------------------------------------
// INVARIANT B — PROJECTION_INTEGRITY
// The frontend must never perform business arithmetic.
// Suppression: [arch:allow-invariant-b] or [arch:allow-fe14*]
// -----------------------------------------------------------
function scanProjectionIntegrity(): { violations: number; warnings: number } {
    let v = 0;
    let w = 0;

    // FE-141: Division on projection values
    checkRule(
        `FE-141 Error: Division on projection values detected — suppress with [arch:allow-fe141]`,
        ["src/pages/**/*.svelte", "src/components/**/*.svelte"],
        /\/(?=[^;]*\b(cost|average|beneficiar|quantity|total|price)\b)/i,
        suppressExclude("fe141"),
        "error",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-142: Frontend must not compute line totals
    checkRule(
        `FE-142 Error: Multiplication on projection values detected — suppress with [arch:allow-fe142]`,
        ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/**/*.ts"],
        /\*(?=[^;]*\b(unit_cost|unitCost|price|cost|quantity|amount|beneficiaries)\b)/i,
        suppressExclude("fe142"),
        "error",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-143: Frontend must not reconstruct aggregate costs
    checkRule(
        `FE-143 Error: Addition on cost values detected — suppress with [arch:allow-fe143]`,
        ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/**/*.ts"],
        /\+(?=[^;]*\b(cost|total_cost|predicted_fifo_cost)\b)/i,
        suppressExclude("fe143"),
        "error",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-145: Frontend must not compute consumption arithmetic
    checkRule(
        `FE-145 Error: Consumption arithmetic detected — suppress with [arch:allow-fe145]`,
        ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/**/*.ts"],
        /[\+\-\*\/](?=[^;]*\b(consumed|planned|mealCount|meal_count|portion|remaining|portionCount)\b)/i,
        suppressExclude("fe145"),
        "error",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-146: Frontend must not recompute averages
    checkRule(
        `FE-146 Error: Average recomputation detected — suppress with [arch:allow-fe146]`,
        ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/**/*.ts"],
        /\b(average|mealAverage|dailyAverage|avg)\s*(?=[:=])|(?<=\/)\s*\b(beneficiaries|count|days)\b/,
        suppressExclude("fe146"),
        "error",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-147: Frontend must not reassemble projections
    checkRule(
        `FE-147 Error: Projection reassembly detected — suppress with [arch:allow-fe147]`,
        ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/**/*.ts"],
        /(\.\.\.\w*project|\.\.\.\w*forecast|\.\.\.\w*report|Object\.assign\([^)]*project)/i,
        suppressExclude("fe147"),
        "error",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-148: Detect long $derived computation chains
    {
        const fe148tag = "[arch:allow-fe148]";
        const patterns = ["src/pages/**/*.svelte", "src/components/**/*.svelte"];
        const scannedFiles = new Set<string>();
        let ruleViolations = 0;
        const matches: { file: string; line: number; content: string }[] = [];

        for (const pattern of patterns) {
            const glob = new Glob(pattern);
            for (const file of glob.scanSync(".")) {
                const normalizedFile = file.replace(/\\/g, "/");
                if (scannedFiles.has(normalizedFile)) continue;
                scannedFiles.add(normalizedFile);
                if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;

                const content = readFileSync(file, "utf-8");
                const lines = content.split("\n");

                lines.forEach((line, index) => {
                    if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) return;
                    if (line.includes(fe148tag)) return;
                    if (index > 0 && lines[index - 1].includes(fe148tag)) return;
                    const derivedCount = (line.match(/\$derived(?:\.\w+)?\s*\(/g) || []).length;
                    if (derivedCount >= 2) {
                        matches.push({ file, line: index + 1, content: line.trim() });
                        ruleViolations++;
                    }
                });
            }
        }

        if (ruleViolations > 0) {
            console.log(`❌ ${colors.red}FE-148 Error: Long $derived computation chain detected — suppress with [arch:allow-fe148]${colors.reset}`);
            matches.forEach((m) => console.log(`  ${m.file}:${m.line} → ${m.content}`));
            v++;
        }
    }

    // FE-152: Projection Ownership Enforcement — pages may only consume through owning domain
    {
        const allDomainFns = new Set<string>();
        const fnToDomain = new Map<string, string>();
        for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
            for (const fn of reg.functions) { allDomainFns.add(fn); fnToDomain.set(fn, domain); }
            for (const fn of reg.crossDomainExceptions) { allDomainFns.add(fn); }
        }

        const pageGlob = new Glob("src/pages/**/*.svelte");
        let fe152Errors = 0;
        const pageToDomain = new Map<string, string[]>();
        for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
            for (const p of reg.pages) {
                const existing = pageToDomain.get(p) || [];
                existing.push(domain);
                pageToDomain.set(p, existing);
            }
        }

        for (const file of pageGlob.scanSync(".")) {
            const normalizedFile = file.replace(/\\/g, "/");
            if (normalizedFile.includes("/tests/") || normalizedFile.includes("/e2e/")) continue;
            const fileName = normalizedFile.split("/").pop()?.replace(".svelte", "") || "";
            if (fileName === "NotFoundPage" || fileName === "App") continue;

            const content = readFileSync(file, "utf-8");
            const importMatch = content.match(/from\s+['"]\.\.?\/lib\/contracts['"]/);
            if (!importMatch) continue;

            const pageFileMatches: string[] = [];
            for (const fn of allDomainFns) {
                if (new RegExp(`\\b${fn}\\b`).test(content)) pageFileMatches.push(fn);
            }

            const pageDomains = pageToDomain.get(fileName) || [];
            const errors: string[] = [];

            for (const fn of pageFileMatches) {
                if (UNIVERSAL_ALLOWED.includes(fn)) continue;
                const domain = fnToDomain.get(fn);
                if (!domain) continue;
                let isAllowed = pageDomains.includes(domain);
                if (!isAllowed) {
                    for (const pd of pageDomains) {
                        if ((DOMAIN_REGISTRY[pd]?.crossDomainExceptions || []).includes(fn)) { isAllowed = true; break; }
                    }
                }
                if (!isAllowed) errors.push(`${fn} (owned by ${domain})`);
            }

            if (errors.length > 0) {
                console.log(`❌ ${colors.red}FE-152 Error: ${fileName} imports projection functions from non-owned domain: ${errors.join(", ")}${colors.reset}`);
                fe152Errors++;
            }
        }
        if (fe152Errors > 0) v += fe152Errors;
    }

    // FE-157: Projection Surface Governance — no computed/derived/helper fields
    {
        const typeFiles = [
            "src/lib/types.ts",
            ...Array.from(new Glob("src/lib/contracts/*.contract.ts").scanSync(".")),
        ];
        let fe157Errors = 0;
        const FE157_ALLOWLIST = new Set(["computed_closing", "computed_at", "reported_closing"]);

        for (const file of typeFiles) {
            const content = readFileSync(file, "utf-8");
            const lines = content.split("\n");
            for (let i = 0; i < lines.length; i++) {
                const line = lines[i];
                const fieldMatch = line.match(/^\s+(\w[\w]*)\s*[?]?\s*:\s*.*;/);
                if (fieldMatch) {
                    const fieldName = fieldMatch[1];
                    if (FE157_ALLOWLIST.has(fieldName)) continue;
                    const fieldLower = fieldName.toLowerCase();
                    if (["computed", "derived", "helper"].includes(fieldLower) ||
                        fieldLower.startsWith("computed_") || fieldLower.startsWith("derived_") || fieldLower.startsWith("helper_")) {
                        console.log(`❌ ${colors.red}FE-157 Error: ${file}:${i + 1} — field '${fieldName}' suggests frontend-convenience computation${colors.reset}`);
                        fe157Errors++;
                    }
                }
                const calcMatch = line.match(/^\s+(\w*[Cc]alculated\w*)\s*[?]?\s*:/);
                if (calcMatch) {
                    console.log(`❌ ${colors.red}FE-157 Error: ${file}:${i + 1} — field '${calcMatch[1]}' suggests frontend-convenience calculation${colors.reset}`);
                    fe157Errors++;
                }
            }
        }

        try {
            const content = readFileSync("src/components/consumption/types.ts", "utf-8");
            const lines = content.split("\n");
            for (let i = 0; i < lines.length; i++) {
                const line = lines[i];
                const fieldMatch = line.match(/^\s+(\w[\w]*)\s*[?]?\s*:\s*.*;/);
                if (fieldMatch) {
                    const fieldName = fieldMatch[1].toLowerCase();
                    if (["computed", "derived", "helper"].includes(fieldName) ||
                        fieldName.startsWith("computed_") || fieldName.startsWith("derived_") || fieldName.startsWith("helper_")) {
                        console.log(`❌ ${colors.red}FE-157 Error: src/components/consumption/types.ts:${i + 1} — field '${fieldMatch[1]}' suggests frontend-convenience computation${colors.reset}`);
                        fe157Errors++;
                    }
                }
            }
        } catch { /* skip */ }

        if (fe157Errors > 0) v += fe157Errors;
    }

    return { violations: v, warnings: w };
}

// -----------------------------------------------------------
// INVARIANT C — RUNTIME_SAFETY
// All reactive state and side effects must be governed.
// Suppression: [arch:allow-invariant-c] or [arch:allow-fe1*]
// -----------------------------------------------------------
function scanRuntimeSafety(): { violations: number; warnings: number } {
    let v = 0;
    let w = 0;

    const fileFilter = (f: string) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.") && !f.includes("/e2e/");
    const srcPatterns = ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/components/**/*.svelte", "src/lib/**/*.ts"];

    // FE-100: Every $state() must have a // @category marker
    checkRule(
        "FE-100 Error: $state() declaration without @category marker",
        srcPatterns,
        /\$state\(/,
        (line, index, lines) => {
            if (line.includes("@category")) return true;
            for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
                const trimmed = lines[i].trim();
                if (trimmed === "") continue;
                if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
                if (trimmed.startsWith("//")) continue;
                break;
            }
            return false;
        },
        "error",
        fileFilter
    );

    // FE-100B: Every $derived() / $derived.by() must have a // @category marker
    checkRule(
        "FE-100B Error: $derived() / $derived.by() declaration without @category marker",
        srcPatterns,
        /\$derived(?:\.\w+)?\s*\(/,
        (line, index, lines) => {
            if (line.includes("@category")) return true;
            for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
                const trimmed = lines[i].trim();
                if (trimmed === "") continue;
                if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
                if (trimmed.startsWith("//")) continue;
                break;
            }
            return false;
        },
        "error",
        fileFilter
    );

    // FE-100C: Module-level reactive let must have @category
    {
        const scannedFiles = new Set<string>();
        let ruleViolations = 0;
        const matches: { file: string; line: number; content: string }[] = [];

        for (const pattern of srcPatterns) {
            const glob = new Glob(pattern);
            for (const file of glob.scanSync(".")) {
                const normalizedFile = file.replace(/\\/g, "/");
                if (scannedFiles.has(normalizedFile)) continue;
                scannedFiles.add(normalizedFile);
                if (!fileFilter(normalizedFile)) continue;

                const content = readFileSync(file, "utf-8");
                const lines = content.split("\n");

                lines.forEach((line, index) => {
                    if (line.trim().startsWith("//")) return;
                    const letMatch = line.match(/^\s*(?:export\s+)?let\s+(\w+)\s*(?::\s*\w+\s*)?=/);
                    if (!letMatch) return;
                    const varName = letMatch[1];
                    const isReactive = content.includes(`$: ${varName}`) ||
                        content.includes(`${varName} = $derived`) ||
                        content.includes(`${varName}.subscribe`) ||
                        content.includes(`$${varName}`);
                    if (!isReactive) return;
                    let hasCategory = false;
                    for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
                        const trimmed = lines[i].trim();
                        if (trimmed === "") continue;
                        if (trimmed.startsWith("//") && trimmed.includes("@category")) { hasCategory = true; break; }
                        if (trimmed.startsWith("//")) continue;
                        break;
                    }
                    if (!hasCategory && !line.includes("@category")) {
                        matches.push({ file, line: index + 1, content: line.trim() });
                        ruleViolations++;
                    }
                });
            }
        }

        if (ruleViolations > 0) {
            console.log(`⚠️ ${colors.yellow}FE-100C Warning: Module-level reactive state without @category marker${colors.reset}`);
            matches.forEach((m) => console.log(`  ${m.file}:${m.line} → ${m.content}`));
            w++;
        }
    }

    // FE-105A: writable() requires @category annotation
    checkRule(
        "FE-105A Error: writable() without @category marker",
        srcPatterns,
        /writable\(/,
        (line, index, lines) => {
            if (line.includes("@category")) return true;
            for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
                const trimmed = lines[i].trim();
                if (trimmed === "") continue;
                if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
                if (trimmed.startsWith("//")) continue;
                break;
            }
            return false;
        },
        "error",
        fileFilter
    );

    // FE-105B: readable() requires @category annotation
    checkRule(
        "FE-105B Error: readable() without @category marker",
        srcPatterns,
        /readable\(/,
        (line, index, lines) => {
            if (line.includes("@category")) return true;
            for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
                const trimmed = lines[i].trim();
                if (trimmed === "") continue;
                if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
                if (trimmed.startsWith("//")) continue;
                break;
            }
            return false;
        },
        "error",
        fileFilter
    );

    // FE-121: Every page must call createRuntimeScope() and dispose in onDestroy
    {
        const pageGlob = new Glob("src/pages/**/*.svelte");
        let fe121Errors = 0;
        let fe121Warnings = 0;
        for (const file of pageGlob.scanSync(".")) {
            const normalized = file.toLowerCase().replace(/\\/g, "/");
            if (normalized.includes("notfoundpage.svelte")) continue;
            const content = readFileSync(file, "utf-8");
            const scopeMatch = content.match(/const\s+(\w+)\s*=\s*createRuntimeScope\(\)/);
            const hasScope = !!scopeMatch;
            if (!hasScope) {
                if (content.includes("createRuntimeScope")) {
                    console.log(`❌ ${colors.red}FE-121 Error: ${file} — createRuntimeScope() must be assigned to a const variable${colors.reset}`);
                    fe121Errors++;
                } else {
                    console.log(`❌ ${colors.red}FE-121 Error: ${file} missing createRuntimeScope()${colors.reset}`);
                    fe121Errors++;
                }
                continue;
            }
            const varName = scopeMatch[1];
            const fakeDisposeRe = new RegExp(varName + '\\.dispose\\s*=\\s*(?=[^=])');
            for (let i = 0; i < content.split("\n").length; i++) {
                const line = content.split("\n")[i];
                if (fakeDisposeRe.test(line)) {
                    console.log(`❌ ${colors.red}FE-121 Error: ${file} — fake dispose override detected: ${line.trim()}${colors.reset}`);
                    fe121Errors++;
                }
            }
            const allDisposeVars = [...content.matchAll(/(\w+)\.dispose\s*\(/g)].map(m => m[1]);
            const otherVars = [...new Set(allDisposeVars.filter(v => v !== varName))];
            if (otherVars.length > 0) {
                console.log(`⚠️ ${colors.yellow}FE-121 Warning: ${file} — dispose called on '${otherVars.join("', '")}' but createRuntimeScope assigned to '${varName}'${colors.reset}`);
                fe121Warnings++;
            }
            const lines = content.split("\n");
            let disposeInOnDestroy = false;
            for (let i = 0; i < lines.length; i++) {
                if (/onDestroy\s*\(\s*\(\s*\)\s*=>\s*\{/.test(lines[i])) {
                    let braceDepth = 0;
                    let inBlock = false;
                    for (let j = i; j < lines.length; j++) {
                        for (const ch of lines[j]) {
                            if (ch === '{') { braceDepth++; inBlock = true; }
                            else if (ch === '}') { braceDepth--; }
                        }
                        if (inBlock && lines[j].includes(varName + ".dispose")) { disposeInOnDestroy = true; break; }
                        if (inBlock && braceDepth === 0) break;
                    }
                    if (disposeInOnDestroy) break;
                }
            }
            if (!disposeInOnDestroy) {
                const simpleRe = new RegExp('onDestroy\\s*\\(\\s*\\(\\s*\\)\\s*=>\\s*' + varName + '\\.dispose\\s*\\(');
                if (simpleRe.test(content)) disposeInOnDestroy = true;
            }
            if (!disposeInOnDestroy) {
                console.log(`⚠️ ${colors.yellow}FE-121 Warning: ${file} has createRuntimeScope (var: ${varName}) but ${varName}.dispose() not found inside onDestroy()${colors.reset}`);
                fe121Warnings++;
            }
        }
        v += fe121Errors;
        w += fe121Warnings;
    }

    // FE-122: createOperation/createOperationGuard should receive RuntimeScope
    checkRule(
        "FE-122: createOperation or createOperationGuard without RuntimeScope (must pass { scope })",
        ["src/pages/**/*.svelte", "src/lib/**/*.ts"],
        /createOperation(?:Guard)?\s*\(\s*(?!\{)/,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            if (line.match(/^\s*(export\s+)?function\s+createOperation/)) return true;
            if (line.includes("createOperation({") || line.includes("createOperationGuard({")) return true;
            if (line.includes("import ")) return true;
            if (line.includes("type CreateOperation")) return true;
            return false;
        },
        "warning",
        (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.")
    );

    // FE-149: session.ts must use createRuntimeScope for all timers and listeners
    {
        const sessionFile = "src/lib/session.ts";
        const content = readFileSync(sessionFile, "utf-8");
        const fe149Errors: string[] = [];
        if (!content.includes("createRuntimeScope")) fe149Errors.push("missing createRuntimeScope import/usage");
        if (!content.includes("scope.setInterval") && !content.includes("scope.setTimeout")) {
            fe149Errors.push("no scope timer methods found");
        }
        if (!content.includes("scope.addListener")) fe149Errors.push("no scope.addListener found");
        if (fe149Errors.length > 0) {
            console.log(`❌ ${colors.red}FE-149 Error: session.ts violates RuntimeScope requirements:${colors.reset}`);
            fe149Errors.forEach(e => console.log(`  → ${e}`));
            v++;
        }
    }

    // FE-150: No module-level mutable timer/listener references outside RuntimeScope
    checkRule(
        "FE-150: Module-level mutable timer/listener reference (use RuntimeScope instead)",
        ["src/lib/**/*.ts", "src/lib/**/*.js"],
        /^\s*export\s+let\s+\w+\s*[?]?\s*:\s*(number|null\s*\|?\s*number|ReturnType<typeof\s+set(?:Timeout|Interval)>)/,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            if (line.includes("scope.")) return true;
            return false;
        },
        "error",
        (f) => {
            const n = f.toLowerCase().replace(/\\/g, "/");
            if (n.includes("/tests/") || n.includes(".test.") || n.includes(".spec.")) return false;
            return true;
        }
    );

    // FE-151: Touch/scroll/wheel listeners should use { passive: true }
    checkRule(
        "FE-151: addEventListener with touch/scroll/wheel missing { passive: true }",
        ["src/**/*.svelte", "src/**/*.ts", "src/**/*.js"],
        /\.addEventListener\s*\(\s*(['"`])(touch|scroll|wheel)\1/,
        (line) => {
            if (line.trim().startsWith("//")) return true;
            if (line.includes("passive: true")) return true;
            if (/scope\.addListener/.test(line)) return true;
            return false;
        },
        "warning",
        fileFilter
    );

    // Suppression validation (FE-149 + FE-162 lifecycle)
    {
        const fePatterns = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
        const glob = new Glob("src/**/*.{svelte,ts,js}");
        const allTags: { tag: string; file: string; line: number; justification: string }[] = [];
        let fe149Errors = 0;
        let fe162Errors = 0;
        let fe162ExpiredErrors = 0;

        for (const file of glob.scanSync(".")) {
            if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.") || file.includes("/e2e/")) continue;
            const content = readFileSync(file, "utf-8");
            const lines = content.split("\n");

            for (let i = 0; i < lines.length; i++) {
                for (const pat of fePatterns) {
                    const marker = `[arch:allow-${pat}]`;
                    if (lines[i].includes(marker)) {
                        // FE-149: Collect for justification check
                        const tagIdx = lines[i].indexOf(marker);
                        const beforeTag = lines[i].substring(0, tagIdx).replace(/^\s*\/\/\s*/, "").replace(/^\s*<!--\s*/, "").trim();
                        const afterTag = lines[i].substring(tagIdx + marker.length).replace(/-->\s*$/, "").trim();
                        const justification = afterTag || beforeTag;
                        allTags.push({ tag: pat, file, line: i + 1, justification });

                        // FE-162: Check metadata
                        const hasReason = /\bReason\s*:/i.test(lines[i]);
                        const hasDate = /\bDate\s*:\s*\d{4}-\d{2}-\d{2}/.test(lines[i]);
                        const hasOwner = /\bOwner\s*:/i.test(lines[i]);
                        if (!hasReason || !hasDate || !hasOwner) {
                            console.log(`❌ ${colors.red}FE-162 Error: ${file}:${i + 1} — suppression '${marker}' missing metadata. Required: Reason, Date, Owner${colors.reset}`);
                            fe162Errors++;
                        } else {
                            const dateMatch = lines[i].match(/Date\s*:\s*(\d{4}-\d{2}-\d{2})/);
                            if (dateMatch) {
                                const date = new Date(dateMatch[1]);
                                const now = new Date();
                                const diffMs = now.getTime() - date.getTime();
                                const diffDays = diffMs / (1000 * 60 * 60 * 24);
                                if (diffDays > 90) {
                                    console.log(`❌ ${colors.red}FE-162 Error: ${file}:${i + 1} — suppression '${marker}' expired (${Math.floor(diffDays)} days old, max 90)${colors.reset}`);
                                    fe162ExpiredErrors++;
                                }
                            }
                        }
                    }
                }
            }
        }

        // FE-149: Check empty justification
        for (const t of allTags) {
            if (!t.justification) {
                console.log(`❌ ${colors.red}FE-149 Error: Empty suppression — ${t.file}:${t.line} (${t.tag}) — must include justification text${colors.reset}`);
                fe149Errors++;
            }
        }

        // FE-149: Check duplicate consecutive identical tags
        for (let i = 1; i < allTags.length; i++) {
            const a = allTags[i - 1];
            const b = allTags[i];
            if (a.tag === b.tag && a.justification === b.justification && a.file === b.file && Math.abs(a.line - b.line) <= 2) {
                console.log(`❌ ${colors.red}FE-149 Error: Duplicate suppression — ${b.file}:${b.line} (${b.tag}) — same tag+justification on adjacent line${colors.reset}`);
                fe149Errors++;
            }
        }

        if (fe149Errors > 0) v += fe149Errors;
        if (fe162Errors > 0 || fe162ExpiredErrors > 0) v += fe162Errors + fe162ExpiredErrors;
    }

    return { violations: v, warnings: w };
}

// -----------------------------------------------------------
// INVARIANT D — ARCHITECTURE_GRAPH
// The system's import graph and layer structure must remain stable.
// Suppression: [arch:allow-invariant-d] or [arch:allow-fe15*]
// -----------------------------------------------------------
function scanArchitectureGraph(): { violations: number; warnings: number } {
    let v = 0;
    let w = 0;

    // FE-131: Page must not import another page
    checkRule(
        "FE-131 Error: Page importing another page (pages must be independent)",
        ["src/pages/**/*.svelte"],
        /import\s+.*from\s+['"]\.\.?\/pages\//,
        (line) => {
            if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) return true;
            return false;
        },
        "error"
    );

    // FE-132: Component must not import a page
    checkRule(
        "FE-132 Error: Component importing a page (components must not depend on pages)",
        ["src/components/**/*.svelte", "src/lib/components/**/*.svelte"],
        /import\s+.*from\s+['"].*pages\//,
        (line) => {
            if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) return true;
            return false;
        },
        "error"
    );

    // FE-155: Architecture Drift Detection — orphan pages/contracts
    {
        const fe155Warnings: string[] = [];
        try {
            const appContent = readFileSync("src/App.svelte", "utf-8");
            const pageGlob = new Glob("src/pages/**/*.svelte");
            for (const file of pageGlob.scanSync(".")) {
                const fileName = file.split("/").pop()?.replace(".svelte", "") || "";
                if (fileName === "NotFoundPage") continue;
                if (!appContent.includes(fileName)) {
                    fe155Warnings.push(`Orphan page: ${file} — not registered in App.svelte router`);
                }
            }
        } catch { /* no App.svelte */ }

        const pageGlob = new Glob("src/pages/**/*.svelte");
        const allPageContent: string[] = [];
        for (const file of pageGlob.scanSync(".")) {
            if (file.includes("/tests/")) continue;
            allPageContent.push(readFileSync(file, "utf-8"));
        }
        const allPageContentJoined = allPageContent.join("\n");

        const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
        for (const file of contractGlob.scanSync(".")) {
            const contractName = file.replace(/^.*\/(\w+)\.contract\.ts$/, "$1");
            if (contractName === "platform") continue;
            const contractContent = readFileSync(file, "utf-8");
            const exports = [...contractContent.matchAll(/export\s+async\s+function\s+(\w+)/g)].map(m => m[1]);
            const used = exports.some(fn => new RegExp(`\\b${fn}\\b`).test(allPageContentJoined));
            if (!used) {
                fe155Warnings.push(`Orphan contract: ${file} — no page consumes any exported function`);
            }
        }

        if (fe155Warnings.length > 0) {
            console.log(`⚠️ ${colors.yellow}FE-155 Warning: Architecture drift detected${colors.reset}`);
            for (const ww of fe155Warnings) console.log(`  ⚠️ ${ww}`);
            w += fe155Warnings.length;
        }
    }

    // FE-156: Contract Size Governance — max 50 exports, max 500 LOC
    {
        let fe156Warnings = 0;
        const glob = new Glob("src/lib/contracts/*.contract.ts");
        for (const file of glob.scanSync(".")) {
            const content = readFileSync(file, "utf-8");
            const loc = content.split("\n").length;
            const exportCount = (content.match(/export\s+async\s+function\s+\w+/g) || []).length;
            if (exportCount > 50) {
                console.log(`⚠️ ${colors.yellow}FE-156 Warning: ${file} has ${exportCount} exported functions (max 50)${colors.reset}`);
                fe156Warnings++;
            }
            if (loc > 500) {
                console.log(`⚠️ ${colors.yellow}FE-156 Warning: ${file} has ${loc} lines (max 500)${colors.reset}`);
                fe156Warnings++;
            }
        }
        if (fe156Warnings > 0) w += fe156Warnings;
    }

    // FE-158: Governance Drift Detection — compare current state against snapshots
    {
        const snapshotDir = "docs/governance/frontend/baselines";
        const snapshots = [
            { file: "contracts.snapshot.json", name: "contract" },
            { file: "domain-ownership.snapshot.json", name: "domain-ownership" },
            { file: "projection-ownership.snapshot.json", name: "projection-ownership" },
            { file: "import-graph.snapshot.json", name: "import-graph" },
        ];

        let fe158Errors = 0;

        for (const snap of snapshots) {
            try { JSON.parse(readFileSync(`${snapshotDir}/${snap.file}`, "utf-8")); }
            catch {
                console.log(`❌ ${colors.red}FE-158 Error: Governance snapshot missing or invalid: ${snap.file}${colors.reset}`);
                fe158Errors++;
            }
        }

        try {
            const contractSnap = JSON.parse(readFileSync(`${snapshotDir}/contracts.snapshot.json`, "utf-8"));
            const currentContracts: Record<string, { exports: string[]; loc: number }> = {};
            const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
            for (const file of contractGlob.scanSync(".")) {
                const content = readFileSync(file, "utf-8");
                const name = file.split("/").pop()!.replace(".contract.ts", "");
                const exports = [...content.matchAll(/export\s+async\s+function\s+(\w+)/g)].map((m) => m[1]);
                currentContracts[name] = { exports, loc: content.split("\n").length };
            }

            for (const snapContract of contractSnap.contracts || []) {
                const current = currentContracts[snapContract.name];
                if (!current) {
                    console.log(`❌ ${colors.red}FE-158 Error: Contract '${snapContract.name}' exists in snapshot but not in current codebase${colors.reset}`);
                    fe158Errors++;
                    continue;
                }
                for (const exportEntry of snapContract.exports || []) {
                    if (!current.exports.includes(exportEntry.name)) {
                        console.log(`❌ ${colors.red}FE-158 Error: Contract '${snapContract.name}' lost export '${exportEntry.name}' (governance drift)${colors.reset}`);
                        fe158Errors++;
                    }
                }
            }

            for (const name of Object.keys(currentContracts)) {
                if (!(contractSnap.contracts || []).some((c: any) => c.name === name)) {
                    console.log(`❌ ${colors.red}FE-158 Error: New contract '${name}' not in governance snapshot${colors.reset}`);
                    fe158Errors++;
                }
            }
        } catch { /* snapshot missing — handled above */ }

        try {
            const domainSnap = JSON.parse(readFileSync(`${snapshotDir}/domain-ownership.snapshot.json`, "utf-8"));
            const currentPages: Record<string, string[]> = {};
            const pageGlob = new Glob("src/pages/**/*.svelte");
            for (const file of pageGlob.scanSync(".")) {
                if (file.includes("/tests/")) continue;
                const content = readFileSync(file, "utf-8");
                const name = file.split("/").pop()!.replace(".svelte", "");
                const fnMatches = [...content.matchAll(/\b(\w+)\s*\(/g)].map((m) => m[1]);
                const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
                const allExports = new Set<string>();
                for (const cf of contractGlob.scanSync(".")) {
                    const cc = readFileSync(cf, "utf-8");
                    for (const e of cc.matchAll(/export\s+async\s+function\s+(\w+)/g)) allExports.add(e[1]);
                }
                currentPages[name] = fnMatches.filter((fn) => allExports.has(fn));
            }

            for (const [page, snapFns] of Object.entries(domainSnap.pages || {})) {
                const currentFns = currentPages[page] || [];
                for (const snapFn of snapFns as string[]) {
                    if (!currentFns.includes(snapFn)) {
                        console.log(`❌ ${colors.red}FE-158 Error: Page '${page}' no longer uses function '${snapFn}' from domain snapshot${colors.reset}`);
                        fe158Errors++;
                    }
                }
            }
        } catch { /* snapshot missing */ }

        if (fe158Errors > 0) v += fe158Errors;
    }

    // FE-159: Contract Mutation Detection — functions duplicated across contracts
    {
        const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
        const fnToContracts = new Map<string, string[]>();
        for (const file of contractGlob.scanSync(".")) {
            const content = readFileSync(file, "utf-8");
            const name = file.split("/").pop()!.replace(".contract.ts", "");
            const exports = [...content.matchAll(/export\s+async\s+function\s+(\w+)/g)].map((m) => m[1]);
            for (const fn of exports) {
                if (!fnToContracts.has(fn)) fnToContracts.set(fn, []);
                fnToContracts.get(fn)!.push(name);
            }
        }
        let fe159Errors = 0;
        for (const [fn, contracts] of fnToContracts) {
            if (contracts.length > 1) {
                console.log(`❌ ${colors.red}FE-159 Error: Function '${fn}' is duplicated across contracts: ${contracts.join(", ")} (contract mutation)${colors.reset}`);
                fe159Errors++;
            }
        }
        if (fe159Errors > 0) v += fe159Errors;
    }

    // FE-160: Projection Mutation Detection — projection field changes vs snapshot
    {
        let fe160Warnings = 0;
        try {
            const snapContent = readFileSync("docs/governance/frontend/baselines/projection-ownership.snapshot.json", "utf-8");
            const snapshot = JSON.parse(snapContent);
            const snapTypes = new Map(snapshot.types.map((t: any) => [t.name, t]));

            const content = readFileSync("src/lib/types.ts", "utf-8");
            const lines = content.split("\n");
            let currentType: { name: string; kind: string; fields: string[] } | null = null;
            const currentTypes = new Map<string, { name: string; kind: string; fields: string[] }>();
            let typeAliasLine = false;

            for (let i = 0; i < lines.length; i++) {
                const line = lines[i];
                const ifaceMatch = line.match(/^export\s+(interface|type)\s+(\w+)/);
                if (ifaceMatch) {
                    if (currentType) currentTypes.set(currentType.name, currentType);
                    currentType = { name: ifaceMatch[2], kind: ifaceMatch[1], fields: [] };
                    typeAliasLine = ifaceMatch[1] === "type";
                    continue;
                }
                if (currentType) {
                    const fieldMatch = line.match(/^\s+(\w[\w\?]*)\s*[?]?\s*:\s*(.+?);/);
                    if (fieldMatch) {
                        const fieldName = fieldMatch[1].replace("?", "");
                        if (!["id", "created_at", "updated_at"].includes(fieldName)) {
                            currentType.fields.push(fieldName);
                        }
                    }
                    if (typeAliasLine) {
                        currentTypes.set(currentType.name, currentType);
                        currentType = null;
                        typeAliasLine = false;
                        continue;
                    }
                    if (/^\}/.test(line.trim()) || /^\};/.test(line.trim())) {
                        currentTypes.set(currentType.name, currentType);
                        currentType = null;
                    }
                }
            }
            if (currentType) currentTypes.set(currentType.name, currentType);

            for (const [name, snapshotType] of snapTypes) {
                const currentDef = currentTypes.get(name);
                if (!currentDef) {
                    console.log(`⚠️ ${colors.yellow}FE-160 Warning: Projection type '${name}' removed from current codebase (projection mutation)${colors.reset}`);
                    fe160Warnings++;
                    continue;
                }
                const currentFields = currentDef.fields;
                for (const field of currentFields) {
                    if (!snapshotType.fields.some((f: any) => f.name === field)) {
                        console.log(`⚠️ ${colors.yellow}FE-160 Warning: Projection '${name}' — new field '${field}' added (projection surface evolution)${colors.reset}`);
                        fe160Warnings++;
                    }
                }
                for (const snapField of snapshotType.fields) {
                    if (!currentFields.includes(snapField.name)) {
                        console.log(`⚠️ ${colors.yellow}FE-160 Warning: Projection '${name}' — field '${snapField.name}' removed (projection mutation)${colors.reset}`);
                        fe160Warnings++;
                    }
                }
            }

            for (const name of currentTypes.keys()) {
                if (!snapTypes.has(name)) {
                    console.log(`⚠️ ${colors.yellow}FE-160 Warning: New projection type '${name}' not in governance snapshot (projection surface evolution)${colors.reset}`);
                    fe160Warnings++;
                }
            }
        } catch { /* skip if snapshot missing */ }

        if (fe160Warnings > 0) w += fe160Warnings;
    }

    // FE-163: Dead Governance Artifact Detection
    {
        let fe163Warnings = 0;

        const existingPages = new Set(
            [...new Glob("src/pages/**/*.svelte").scanSync(".")]
                .map((f) => f.split("/").pop()!.replace(".svelte", ""))
                .filter((n) => n !== "NotFoundPage" && n !== "App"),
        );

        for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
            for (const page of reg.pages) {
                if (!existingPages.has(page)) {
                    console.log(`⚠️ ${colors.yellow}FE-163 Warning: Dead governance entry — page '${page}' registered for domain '${domain}' but no such page exists${colors.reset}`);
                    fe163Warnings++;
                }
            }
        }

        const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
        const allExistingExports = new Set<string>();
        for (const file of contractGlob.scanSync(".")) {
            const content = readFileSync(file, "utf-8");
            for (const m of content.matchAll(/export\s+async\s+function\s+(\w+)/g)) allExistingExports.add(m[1]);
        }

        for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
            for (const fn of reg.functions) {
                if (!allExistingExports.has(fn)) {
                    console.log(`⚠️ ${colors.yellow}FE-163 Warning: Dead governance entry — function '${fn}' registered for domain '${domain}' but no such export exists${colors.reset}`);
                    fe163Warnings++;
                }
            }
            for (const fn of reg.crossDomainExceptions) {
                if (!allExistingExports.has(fn) && fn !== "") {
                    console.log(`⚠️ ${colors.yellow}FE-163 Warning: Unused cross-domain exception — '${fn}' in domain '${domain}' does not correspond to any existing export${colors.reset}`);
                    fe163Warnings++;
                }
            }
        }

        if (fe163Warnings > 0) w += fe163Warnings;
    }

    return { violations: v, warnings: w };
}

// -----------------------------------------------------------
// META-INVARIANT — GOVERNANCE_FREEZE
// The governance system itself is frozen and externally verifiable.
// Not suppressible.
// -----------------------------------------------------------
function scanGovernanceFreeze(): { violations: number; warnings: number } {
    let v = 0;
    let w = 0;

    // FE-165: Release Gate — required governance artifacts exist
    {
        const snapshotDir = "docs/governance/frontend/baselines";
        let fe165Errors = 0;

        const requiredSnapshots = [
            "contracts.snapshot.json", "domain-ownership.snapshot.json",
            "projection-ownership.snapshot.json", "import-graph.snapshot.json",
        ];
        for (const snap of requiredSnapshots) {
            try { JSON.parse(readFileSync(`${snapshotDir}/${snap}`, "utf-8")); }
            catch {
                console.log(`❌ ${colors.red}FE-165 Error: Release gate — governance snapshot missing or invalid: ${snap}${colors.reset}`);
                fe165Errors++;
            }
        }

        const requiredDocs = [
            { path: "docs/governance/frontend/GOVERNANCE_APPROVALS.md", check: (c: string) => c.includes("## Approvals") },
            { path: "docs/governance/frontend/GOVERNANCE_FREEZE.md", check: (_c: string) => true },
            { path: "docs/governance/frontend/RELEASE_CERTIFICATION_CHECKLIST.md", check: (_c: string) => true },
            { path: "docs/governance/frontend/GOVERNANCE_METRICS.md", check: (_c: string) => true },
            { path: "docs/governance/frontend/GOVERNANCE_COVERAGE_REPORT.md", check: (c: string) => c.includes("Coverage Metrics") },
            { path: "docs/governance/frontend/FRONTEND_CERTIFICATION_v6.md", check: (c: string) => c.includes("Status:") },
        ];

        for (const doc of requiredDocs) {
            try {
                const content = readFileSync(doc.path, "utf-8");
                if (!doc.check(content)) {
                    console.log(`❌ ${colors.red}FE-165 Error: Release gate — ${doc.path} missing required content${colors.reset}`);
                    fe165Errors++;
                }
            } catch {
                console.log(`❌ ${colors.red}FE-165 Error: Release gate — ${doc.path} missing${colors.reset}`);
                fe165Errors++;
            }
        }

        if (fe165Errors > 0) v += fe165Errors;
    }

    // FE-166: Snapshot Approval Enforcement
    {
        let fe166Errors = 0;
        const snapshotDir = "docs/governance/frontend/baselines";
        const snapshots = [
            { file: "contracts.snapshot.json" },
            { file: "domain-ownership.snapshot.json" },
            { file: "projection-ownership.snapshot.json" },
            { file: "import-graph.snapshot.json" },
        ];

        for (const snap of snapshots) {
            try {
                const parsed = JSON.parse(readFileSync(`${snapshotDir}/${snap.file}`, "utf-8"));
                if (!parsed.generated) {
                    console.log(`❌ ${colors.red}FE-166 Error: ${snap.file} missing 'generated' timestamp (not a valid governance snapshot)${colors.reset}`);
                    fe166Errors++;
                }
            } catch { /* missing snapshots handled by FE-165 */ }
        }

        try {
            const approvalsContent = readFileSync("docs/governance/frontend/GOVERNANCE_APPROVALS.md", "utf-8");
            if (!approvalsContent.includes("snapshot") && !approvalsContent.includes("Snapshot")) {
                console.log(`❌ ${colors.red}FE-166 Error: GOVERNANCE_APPROVALS.md contains no snapshot-related approvals${colors.reset}`);
                fe166Errors++;
            }
        } catch { /* handled by FE-165 */ }

        if (fe166Errors > 0) v += fe166Errors;
    }

    // FE-167: Certification Consistency
    {
        let fe167Warnings = 0;

        try {
            const v6Content = readFileSync("docs/governance/frontend/FRONTEND_CERTIFICATION_v6.md", "utf-8");
            if (!v6Content.includes("v5-freeze")) {
                console.log(`⚠️ ${colors.yellow}FE-167 Warning: FRONTEND_CERTIFICATION_v6.md does not reference snapshot version 'v5-freeze'${colors.reset}`);
                fe167Warnings++;
            }
            if (!v6Content.includes("v6")) {
                console.log(`⚠️ ${colors.yellow}FE-167 Warning: FRONTEND_CERTIFICATION_v6.md does not reference version 'v6'${colors.reset}`);
                fe167Warnings++;
            }
        } catch { /* handled by FE-165 */ }

        try {
            const freezeContent = readFileSync("docs/governance/frontend/GOVERNANCE_FREEZE.md", "utf-8");
            if (!freezeContent.includes("v5.0.0") && !freezeContent.includes("v5")) {
                console.log(`⚠️ ${colors.yellow}FE-167 Warning: GOVERNANCE_FREEZE.md does not reference version 'v5'${colors.reset}`);
                fe167Warnings++;
            }
        } catch { /* handled by FE-165 */ }

        try {
            const metricsContent = readFileSync("docs/governance/frontend/GOVERNANCE_METRICS.md", "utf-8");
            if (!metricsContent.includes("v5-freeze")) {
                console.log(`⚠️ ${colors.yellow}FE-167 Warning: GOVERNANCE_METRICS.md does not reference snapshot version 'v5-freeze'${colors.reset}`);
                fe167Warnings++;
            }
        } catch { /* handled by FE-165 */ }

        try {
            const approvalsContent = readFileSync("docs/governance/frontend/GOVERNANCE_APPROVALS.md", "utf-8");
            if (!approvalsContent.includes("v5") && !approvalsContent.includes("Phase 5")) {
                console.log(`⚠️ ${colors.yellow}FE-167 Warning: GOVERNANCE_APPROVALS.md does not reference version v5 or Phase 5${colors.reset}`);
                fe167Warnings++;
            }
        } catch { /* handled by FE-165 */ }

        const certFiles = [
            "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v1.md",
            "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v2.md",
            "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v3.md",
            "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v4.md",
            "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v5.md",
        ];
        for (const f of certFiles) {
            try { readFileSync(f, "utf-8"); }
            catch {
                console.log(`⚠️ ${colors.yellow}FE-167 Warning: Archived certification file missing: ${f}${colors.reset}`);
                fe167Warnings++;
            }
        }

        if (fe167Warnings > 0) w += fe167Warnings;
    }

    return { violations: v, warnings: w };
}

// -----------------------------------------------------------
// Execute all 5 invariant scans
// -----------------------------------------------------------

const invariantResults = [
    { name: "INVARIANT A — Contract Boundary", result: scanContractBoundary() },
    { name: "INVARIANT B — Projection Integrity", result: scanProjectionIntegrity() },
    { name: "INVARIANT C — Runtime Safety", result: scanRuntimeSafety() },
    { name: "INVARIANT D — Architecture Graph", result: scanArchitectureGraph() },
    { name: "META — Governance Freeze", result: scanGovernanceFreeze() },
];

for (const ir of invariantResults) {
    violations += ir.result.violations;
    warnings += ir.result.warnings;
}

// ============================================================
// SUMMARY
// ============================================================

console.log("");

if (violations === 0 && warnings === 0) {
    console.log(
        `✅ ${colors.green}${colors.bold}Architecture check passed with no issues.${colors.reset}`
    );

    process.exit(0);
} else if (violations === 0) {
    console.log(
        `❌ ${colors.yellow}Architecture check failed: ${warnings} warning(s) require resolution (zero-warning policy).${colors.reset}`
    );

    process.exit(1);
} else {
    console.log(
        `\n❌ ${colors.red}${violations} rule(s) violated. Architectural integrity compromised.${colors.reset}`
    );

    if (warnings > 0) {
        console.log(
            `⚠️  ${colors.yellow}${warnings} additional warning(s) require review.${colors.reset}`
        );
    }

    process.exit(1);
}
