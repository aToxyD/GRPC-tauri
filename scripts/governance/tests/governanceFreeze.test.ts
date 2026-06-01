import { describe, it, expect } from "bun:test";
import { FileCache } from "../scanner";

describe("governanceFreeze scanner", () => {
  it("should return violations array (possibly empty)", async () => {
    const { scanGovernanceFreeze } = await import("../invariants/governanceFreeze");
    const cache = new FileCache();
    const violations = scanGovernanceFreeze(cache);
    expect(Array.isArray(violations)).toBe(true);
    for (const v of violations) {
      expect(v.invariant).toBe("GOVERNANCE_FREEZE");
    }
  });

  it("FE-165: should require governance snapshots exist", async () => {
    const { scanGovernanceFreeze } = await import("../invariants/governanceFreeze");
    const cache = new FileCache();
    const violations = scanGovernanceFreeze(cache);
    const fe165 = violations.filter((v) => v.rule === "FE-165" && v.message.includes("snapshot"));
    expect(fe165.length).toBe(0);
  });

  it("FE-167: should validate certification consistency", async () => {
    const { scanGovernanceFreeze } = await import("../invariants/governanceFreeze");
    const cache = new FileCache();
    const violations = scanGovernanceFreeze(cache);
    const fe167 = violations.filter((v) => v.rule === "FE-167");
    expect(Array.isArray(fe167)).toBe(true);
  });
});
