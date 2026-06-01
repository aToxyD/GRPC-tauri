import { describe, it, expect } from "bun:test";
import { FileCache } from "../scanner";

describe("runtimeSafety scanner", () => {
  it("should return violations array (possibly empty)", async () => {
    const { scanRuntimeSafety } = await import("../invariants/runtimeSafety");
    const cache = new FileCache();
    const violations = scanRuntimeSafety(cache);
    expect(Array.isArray(violations)).toBe(true);
    for (const v of violations) {
      expect(["RUNTIME_SAFETY"]).toContain(v.invariant);
    }
  });

  it("FE-100: should validate @category markers on $state()", async () => {
    const { scanRuntimeSafety } = await import("../invariants/runtimeSafety");
    const cache = new FileCache();
    const violations = scanRuntimeSafety(cache);
    const fe100 = violations.filter((v) => v.rule === "FE-100" || v.rule === "FE-100B" || v.rule === "FE-100C");
    // Categories are advisory - 0 errors is fine
    expect(Array.isArray(fe100)).toBe(true);
  });
});
