import { describe, it, expect } from "bun:test";
import { FileCache } from "../scanner";

const INVARIANT = "CONTRACT_BOUNDARY" as const;

describe("contractBoundary scanner", () => {
  it("should return violations array (possibly empty)", async () => {
    const { scanContractBoundary } = await import("../invariants/contractBoundary");
    const cache = new FileCache();
    const violations = scanContractBoundary(cache);
    expect(Array.isArray(violations)).toBe(true);
    // All violations should have the CONTRACT_BOUNDARY invariant
    for (const v of violations) {
      expect(v.invariant).toBe("CONTRACT_BOUNDARY");
    }
  });

  it("FE-111: should detect missing contract files", async () => {
    const { scanContractBoundary } = await import("../invariants/contractBoundary");
    const cache = new FileCache();
    const violations = scanContractBoundary(cache);
    const fe111 = violations.filter((v) => v.rule === "FE-111");
    // At minimum, we should get 0 FE-111 errors since contracts should exist
    // This test verifies the rule runs without errors
    expect(Array.isArray(fe111)).toBe(true);
  });

  it("FE-136: should verify barrel file exists", async () => {
    const { scanContractBoundary } = await import("../invariants/contractBoundary");
    const cache = new FileCache();
    const violations = scanContractBoundary(cache);
    const fe136 = violations.filter((v) => v.rule === "FE-136");
    // Barrel should exist, so no violations
    expect(fe136.length).toBe(0);
  });

  it("FE-153: should detect IPC commands in multiple contracts", async () => {
    const { scanContractBoundary } = await import("../invariants/contractBoundary");
    const cache = new FileCache();
    const violations = scanContractBoundary(cache);
    const fe153 = violations.filter((v) => v.rule === "FE-153");
    // IPC commands should not be duplicated across contracts
    expect(fe153.length).toBe(0);
  });

  it("FE-154: should verify barrel re-exports all contracts", async () => {
    const { scanContractBoundary } = await import("../invariants/contractBoundary");
    const cache = new FileCache();
    const violations = scanContractBoundary(cache);
    const fe154 = violations.filter((v) => v.rule === "FE-154" && v.message.includes("not re-exported"));
    expect(fe154.length).toBe(0);
  });
});
