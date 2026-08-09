import { readFileSync } from "fs";
import { Glob } from "bun";
import type { Violation } from "./types";
import type { FileCache } from "./scanner";

export function suppressExclude(tag: string): (line: string, index: number, lines: string[]) => boolean {
  return (line: string, index: number, lines: string[]) => {
    const marker = `[arch:allow-${tag}]`;
    if (line.includes(marker)) return true;
    if (index > 0 && lines[index - 1].includes(marker)) return true;
    return false;
  };
}

export interface SuppressionEntry {
  tag: string;
  file: string;
  line: number;
  justification: string;
  reason?: string;
  date?: string;
  owner?: string;
}

export function collectSuppressions(
  cache: FileCache,
  patterns: string[],
  tagPatterns: string[],
): SuppressionEntry[] {
  const entries: SuppressionEntry[] = [];

  for (const pattern of patterns) {
    for (const file of new Glob(pattern).scanSync(".")) {
      if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.") || file.includes("/e2e/")) continue;
      const content = cache.get(file);
      const lines = content.split("\n");

      for (let i = 0; i < lines.length; i++) {
        for (const pat of tagPatterns) {
          const marker = `[arch:allow-${pat}]`;
          if (lines[i].includes(marker)) {
            const tagIdx = lines[i].indexOf(marker);
            const beforeTag = lines[i].substring(0, tagIdx).replace(/^\s*\/\/\s*/, "").replace(/^\s*<!--\s*/, "").trim();
            const afterTag = lines[i].substring(tagIdx + marker.length).replace(/-->\s*$/, "").trim();
            const justification = afterTag || beforeTag;

            const hasReason = /\bReason\s*:/i.test(lines[i]);
            const hasDate = /\bDate\s*:\s*\d{4}-\d{2}-\d{2}/.test(lines[i]);
            const hasOwner = /\bOwner\s*:/i.test(lines[i]);

            entries.push({
              tag: pat,
              file,
              line: i + 1,
              justification,
              reason: hasReason ? lines[i] : undefined,
              date: hasDate ? (lines[i].match(/Date\s*:\s*(\d{4}-\d{2}-\d{2})/)?.[1]) : undefined,
              owner: hasOwner ? lines[i] : undefined,
            });
          }
        }
      }
    }
  }

  return entries;
}

/**
 * Permanent-exception registry — derived from `docs/architecture/adr_exception_registry.md`
 * so the validator is bound to the real registry, not a disconnected manual list.
 * A suppression is only exempt when its `Permanent: ADR-NNNN` marker resolves to an ADR
 * registered there as PERMANENT — the marker string alone never grants permanence.
 */
const PERMANENT_REGISTRY_FILE = "docs/architecture/adr_exception_registry.md";

export function getPermanentAdrs(): Set<string> {
  try {
    const content = readFileSync(PERMANENT_REGISTRY_FILE, "utf-8");
    const adrs = new Set<string>();
    const re = /PERMANENT \(ADR-(\d{4})\)/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(content)) !== null) adrs.add(m[1]);
    return adrs;
  } catch {
    return new Set();
  }
}

/**
 * True when the entry declares a `Permanent: ADR-NNNN` marker whose ADR is registered
 * in the exception registry as PERMANENT. Used by the validator to skip the 90-day check.
 */
function isPermanent(entry: SuppressionEntry): boolean {
  const match = (entry.justification || "").match(/Permanent\s*:\s*ADR-(\d{4})/i);
  if (!match) return false;
  return getPermanentAdrs().has(match[1]);
}

export function validateSuppressionMetadata(
  entries: SuppressionEntry[],
  invariant: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];
  const seen = new Set<string>();

  for (const entry of entries) {
    // FE-149: Empty justification
    if (!entry.justification) {
      violations.push({
        invariant,
        severity: "ERROR",
        file: entry.file,
        line: entry.line,
        message: `Empty suppression — (${entry.tag}) — must include justification text`,
        rule: "FE-149",
      });
    }

    // FE-149: Duplicate consecutive identical tags
    const key = `${entry.file}:${entry.tag}:${entry.justification}`;
    if (seen.has(key)) {
      violations.push({
        invariant,
        severity: "ERROR",
        file: entry.file,
        line: entry.line,
        message: `Duplicate suppression — (${entry.tag}) — same tag+justification on adjacent line`,
        rule: "FE-149",
      });
    }
    seen.add(key);

    // FE-162: Missing metadata
    if (!entry.reason || !entry.date || !entry.owner) {
      const missing: string[] = [];
      if (!entry.reason) missing.push("Reason");
      if (!entry.date) missing.push("Date");
      if (!entry.owner) missing.push("Owner");
      violations.push({
        invariant,
        severity: "ERROR",
        file: entry.file,
        line: entry.line,
        message: `Suppression '[arch:allow-${entry.tag}]' missing metadata. Required: ${missing.join(", ")}`,
        rule: "FE-162",
      });
    }

    // FE-162: Expired (> 90 days) — skipped for registered PERMANENT exceptions
    const permanentAdrs = getPermanentAdrs();
    const just = entry.justification || "";
    const permanentAdr = just.match(/Permanent\s*:\s*(?:ADR-)?(\d{4})/i)?.[1];
    const declaredPermanent = /Permanent\s*:/i.test(just);

    // FE-162: A Permanent marker must resolve to a registered PERMANENT ADR.
    if (declaredPermanent && !permanentAdr) {
      violations.push({
        invariant,
        severity: "ERROR",
        file: entry.file,
        line: entry.line,
        message: `Suppression '[arch:allow-${entry.tag}]' declares Permanent but has no ADR reference`,
        rule: "FE-162",
      });
    } else if (declaredPermanent && !permanentAdrs.has(permanentAdr!)) {
      violations.push({
        invariant,
        severity: "ERROR",
        file: entry.file,
        line: entry.line,
        message: `Suppression '[arch:allow-${entry.tag}]' declares Permanent: ADR-${permanentAdr} but that ADR is not registered as PERMANENT`,
        rule: "FE-162",
      });
    }

    const isExemptPermanent = declaredPermanent && permanentAdr !== undefined && permanentAdrs.has(permanentAdr);

    if (entry.date && !isExemptPermanent) {
      const date = new Date(entry.date);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffDays = diffMs / (1000 * 60 * 60 * 24);
      if (diffDays > 90) {
        violations.push({
          invariant,
          severity: "ERROR",
          file: entry.file,
          line: entry.line,
          message: `Suppression '[arch:allow-${entry.tag}]' expired (${Math.floor(diffDays)} days old, max 90)`,
          rule: "FE-162",
        });
      }
    }
  }

  return violations;
}
