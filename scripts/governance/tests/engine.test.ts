import { describe, it, expect } from "bun:test";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

// ----------------------------------------------------------
// types.ts
// ----------------------------------------------------------
describe("types", () => {
  it("should define InvariantName types", async () => {
    const mod = await import("../types");
    const violation: mod.Violation = {
      invariant: "CONTRACT_BOUNDARY",
      severity: "ERROR",
      file: "test.ts",
      line: 1,
      message: "test",
      rule: "FE-111",
    };
    expect(violation.invariant).toBe("CONTRACT_BOUNDARY");
    expect(violation.severity).toBe("ERROR");
  });

  it("should accept all invariant names", async () => {
    const mod = await import("../types");
    const names: mod.InvariantName[] = [
      "CONTRACT_BOUNDARY",
      "PROJECTION_INTEGRITY",
      "RUNTIME_SAFETY",
      "ARCHITECTURE_GRAPH",
      "GOVERNANCE_FREEZE",
    ];
    for (const name of names) {
      const v: mod.Violation = {
        invariant: name,
        severity: "WARNING",
        file: "",
        message: "",
      };
      expect(v.invariant).toBe(name);
    }
  });
});

// ----------------------------------------------------------
// scanner.ts — FileCache
// ----------------------------------------------------------
describe("FileCache", () => {
  it("should cache file content", async () => {
    const { FileCache } = await import("../scanner");
    const cache = new FileCache();
    const content = cache.get("scripts/check_arch.ts");
    expect(content.length).toBeGreaterThan(0);
    expect(cache.hits).toBe(0);
    expect(cache.misses).toBe(1);
    expect(cache.files).toBe(1);

    // Second read should hit cache
    const content2 = cache.get("scripts/check_arch.ts");
    expect(content2).toBe(content);
    expect(cache.hits).toBe(1);
    expect(cache.misses).toBe(1);
  });

  it("should track multiple files", async () => {
    const { FileCache } = await import("../scanner");
    const cache = new FileCache();
    cache.get("scripts/check_arch.ts");
    cache.get("src/lib/types.ts");
    expect(cache.files).toBe(2);
    expect(cache.misses).toBe(2);
  });

  it("exists should return true for existing files", async () => {
    const { FileCache } = await import("../scanner");
    const cache = new FileCache();
    expect(cache.exists("scripts/check_arch.ts")).toBe(true);
  });

  it("exists should return false for non-existing files", async () => {
    const { FileCache } = await import("../scanner");
    const cache = new FileCache();
    expect(cache.exists("nonexistent.file")).toBe(false);
  });
});

// ----------------------------------------------------------
// scanner.ts — DOMAIN_REGISTRY
// ----------------------------------------------------------
describe("DOMAIN_REGISTRY", () => {
  it("should define all expected domains", async () => {
    const { DOMAIN_REGISTRY, UNIVERSAL_ALLOWED } = await import("../scanner");
    const expectedDomains = [
      "consumption", "inventory", "orders", "report", "fiscal",
      "sync", "backup", "dashboard", "observability", "audit",
      "metrics", "session", "user",
    ];
    for (const d of expectedDomains) {
      expect(DOMAIN_REGISTRY[d]).toBeDefined();
    }
    expect(UNIVERSAL_ALLOWED).toContain("getSettings");
  });

  it("each domain should have contract, pages, functions, crossDomainExceptions", async () => {
    const { DOMAIN_REGISTRY } = await import("../scanner");
    for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
      expect(reg.contract).toBeDefined();
      expect(Array.isArray(reg.pages)).toBe(true);
      expect(Array.isArray(reg.functions)).toBe(true);
      expect(Array.isArray(reg.crossDomainExceptions)).toBe(true);
    }
  });
});

// ----------------------------------------------------------
// suppression.ts
// ----------------------------------------------------------
describe("suppressExclude", () => {
  it("should create exclude function that matches tag on same line", async () => {
    const { suppressExclude } = await import("../suppression");
    const exclude = suppressExclude("fe141");
    const lines = ["some code [arch:allow-fe141] more code", "other line"];
    expect(exclude(lines[0], 0, lines)).toBe(true);
    expect(exclude(lines[1], 1, lines)).toBe(true); // previous line also excluded
  });

  it("should match tag on previous line", async () => {
    const { suppressExclude } = await import("../suppression");
    const exclude = suppressExclude("fe142");
    const lines = ["// [arch:allow-fe142]", "some code"];
    expect(exclude(lines[1], 1, lines)).toBe(true);
  });
});

describe("collectSuppressions", () => {
  it("should collect suppression tags from files", async () => {
    const { FileCache } = await import("../scanner");
    const { collectSuppressions } = await import("../suppression");
    const cache = new FileCache();
    const entries = collectSuppressions(cache, ["src/lib/types.ts"], ["created-at"]);
    expect(Array.isArray(entries)).toBe(true);
  });
});

describe("validateSuppressionMetadata", () => {
  it("should flag empty justification", async () => {
    const { validateSuppressionMetadata } = await import("../suppression");
    const violations = validateSuppressionMetadata(
      [
        {
          tag: "fe141",
          file: "test.ts",
          line: 1,
          justification: "",
        },
      ],
      "RUNTIME_SAFETY",
    );
    expect(violations.length).toBeGreaterThan(0);
    expect(violations[0].severity).toBe("ERROR");
  });

  it("should flag duplicate consecutive tags", async () => {
    const { validateSuppressionMetadata } = await import("../suppression");
    const violations = validateSuppressionMetadata(
      [
        { tag: "fe141", file: "test.ts", line: 1, justification: "same reason" },
        { tag: "fe141", file: "test.ts", line: 2, justification: "same reason" },
      ],
      "RUNTIME_SAFETY",
    );
    const dupViolations = violations.filter((v) => v.message.includes("Duplicate"));
    expect(dupViolations.length).toBeGreaterThan(0);
  });

  it("should flag missing metadata", async () => {
    const { validateSuppressionMetadata } = await import("../suppression");
    const violations = validateSuppressionMetadata(
      [
        {
          tag: "fe141",
          file: "test.ts",
          line: 1,
          justification: "some reason",
          // no reason, date, owner
        },
      ],
      "RUNTIME_SAFETY",
    );
    const metaViolations = violations.filter((v) => v.message.includes("missing metadata"));
    expect(metaViolations.length).toBeGreaterThan(0);
  });

  it("should flag expired suppressions (> 90 days)", async () => {
    const { validateSuppressionMetadata } = await import("../suppression");
    const oldDate = new Date();
    oldDate.setDate(oldDate.getDate() - 100);
    const dateStr = oldDate.toISOString().split("T")[0];

    const violations = validateSuppressionMetadata(
      [
        {
          tag: "fe141",
          file: "test.ts",
          line: 1,
          justification: "old suppression",
          date: dateStr,
          reason: "needs config migration",
          owner: "dev-team",
        },
      ],
      "RUNTIME_SAFETY",
    );
    const expiredViolations = violations.filter((v) => v.message.includes("expired"));
    expect(expiredViolations.length).toBeGreaterThan(0);
  });
});

// ----------------------------------------------------------
// reporter.ts
// ----------------------------------------------------------
describe("reporter", () => {
  it("should handle empty violations list without errors", async () => {
    // We can't easily test process.exit, but we can verify the function runs
    const { reportViolations } = await import("../reporter");
    // Save original exit
    const origExit = process.exit;
    let exitCode: number | null = null;
    process.exit = ((code?: number) => { exitCode = code ?? 0; }) as any;

    reportViolations([], {
      filesScanned: 0,
      cacheHits: 0,
      cacheMisses: 0,
      invariantTimings: [],
    });

    expect(exitCode).toBe(0);
    process.exit = origExit;
  });
});

// ----------------------------------------------------------
// engine.ts
// ----------------------------------------------------------
describe("engine", () => {
  it("should export runGovernanceAudit function", async () => {
    const mod = await import("../engine");
    expect(typeof mod.runGovernanceAudit).toBe("function");
  });
});
