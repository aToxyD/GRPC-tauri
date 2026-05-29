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

                if (regex.test(line) && !excludeLines(line)) {
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
    (line) => line.trim().startsWith("//")
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
    (line) => {
        // Skip ordinary comment lines
        if (line.trim().startsWith("//")) return true;
        // Skip lines with the explicit inline suppression tag:
        //   [arch:allow-unwrap-or]  — must also include a rationale comment
        if (line.includes("[arch:allow-unwrap-or]")) return true;
        return false;
    },
    "warning",
    (filename) => /sync|parse|service/.test(filename)
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
    (line) => {
        if (line.trim().startsWith("//")) return true;
        if (line.includes("[arch:allow-created-at]")) return true;
        return false;
    },
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
