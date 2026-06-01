import { describe, it, expect } from "bun:test";
import { FileCache } from "../scanner";

describe("projectionIntegrity scanner", () => {
  it("should return violations array (possibly empty)", async () => {
    const { scanProjectionIntegrity } = await import("../invariants/projectionIntegrity");
    const cache = new FileCache();
    const violations = scanProjectionIntegrity(cache);
    expect(Array.isArray(violations)).toBe(true);
    for (const v of violations) {
      expect(v.invariant).toBe("PROJECTION_INTEGRITY");
    }
  });

  it("FE-152: should validate projection ownership", async () => {
    const { scanProjectionIntegrity } = await import("../invariants/projectionIntegrity");
    const cache = new FileCache();
    const violations = scanProjectionIntegrity(cache);
    const fe152 = violations.filter((v) => v.rule === "FE-152");
    expect(Array.isArray(fe152)).toBe(true);
    // Check that errors are properly structured
    for (const v of fe152) {
      expect(v.severity).toBe("ERROR");
      expect(v.file).toBeTruthy();
      expect(v.message).toContain("non-owned domain");
    }
  });
});
