import { describe, it, expect } from "bun:test";
import { FileCache } from "../scanner";

describe("architectureGraph scanner", () => {
  it("should return violations array (possibly empty)", async () => {
    const { scanArchitectureGraph } = await import("../invariants/architectureGraph");
    const cache = new FileCache();
    const violations = scanArchitectureGraph(cache);
    expect(Array.isArray(violations)).toBe(true);
    for (const v of violations) {
      expect(v.invariant).toBe("ARCHITECTURE_GRAPH");
    }
  });

  it("FE-131: pages should not import other pages", async () => {
    const { scanArchitectureGraph } = await import("../invariants/architectureGraph");
    const cache = new FileCache();
    const violations = scanArchitectureGraph(cache);
    const fe131 = violations.filter((v) => v.rule === "FE-131");
    expect(fe131.length).toBe(0);
  });

  it("FE-132: components should not import pages", async () => {
    const { scanArchitectureGraph } = await import("../invariants/architectureGraph");
    const cache = new FileCache();
    const violations = scanArchitectureGraph(cache);
    const fe132 = violations.filter((v) => v.rule === "FE-132");
    expect(fe132.length).toBe(0);
  });

  it("FE-163: dead governance entry detection should be properly structured", async () => {
    const { scanArchitectureGraph } = await import("../invariants/architectureGraph");
    const cache = new FileCache();
    const violations = scanArchitectureGraph(cache);
    const fe163 = violations.filter((v) => v.rule === "FE-163");
    for (const v of fe163) {
      expect(v.severity).toBe("WARNING");
      expect(v.message).toMatch(/Dead governance|Unused cross-domain/);
    }
  });
});
