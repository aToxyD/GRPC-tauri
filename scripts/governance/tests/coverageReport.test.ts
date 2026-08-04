import { describe, it, expect } from "bun:test";
import { FileCache } from "../scanner";

describe("coverageReport observability module", () => {
  it("should produce a coverage report with a Coverage Metrics section", async () => {
    const { computeCoverageReport, generateCoverageReport } = await import(
      "../observability/coverageReport"
    );
    const data = computeCoverageReport(new FileCache());
    expect(Array.isArray(data.buckets)).toBe(true);
    expect(data.buckets.length).toBeGreaterThan(0);
    expect(data.overallCoverage).toBeGreaterThanOrEqual(0);
    expect(data.overallCoverage).toBeLessThanOrEqual(100);

    const report = generateCoverageReport(data);
    expect(report).toContain("## Coverage Metrics");
    for (const bucket of data.buckets) {
      expect(report).toContain(bucket.label);
    }
  });

  it("FE-165: generated report must satisfy the release-gate content check", async () => {
    const { computeCoverageReport, generateCoverageReport } = await import(
      "../observability/coverageReport"
    );
    const report = generateCoverageReport(computeCoverageReport(new FileCache()));
    expect(report.includes("Coverage Metrics")).toBe(true);
  });

  it("should report zero IPC isolation violations (src/main.ts is a sanctioned exception)", async () => {
    const { computeCoverageReport } = await import("../observability/coverageReport");
    const data = computeCoverageReport(new FileCache());
    expect(data.ipcIsolationViolations).toBe(0);
  });
});
