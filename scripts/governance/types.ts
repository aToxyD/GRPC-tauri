export type InvariantName =
  | "CONTRACT_BOUNDARY"
  | "PROJECTION_INTEGRITY"
  | "RUNTIME_SAFETY"
  | "ARCHITECTURE_GRAPH"
  | "GOVERNANCE_FREEZE";

export interface Violation {
  invariant: InvariantName;
  severity: "ERROR" | "WARNING";
  file: string;
  line?: number;
  message: string;
  rule?: string;
}

export interface InvariantTiming {
  name: string;
  durationMs: number;
}

export interface ExecutionMetrics {
  filesScanned: number;
  cacheHits: number;
  cacheMisses: number;
  invariantTimings: InvariantTiming[];
}
