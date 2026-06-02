export interface TelemetryData {
  invariantCount: number;
  snapshotCount: number;
  approvalCount: number;
  suppressionCount: number;
  expiredSuppressionCount: number;
  contractCount: number;
  pageCount: number;
  componentCount: number;
  ruleCount: number;
}

export interface HealthScoreData {
  architectureGraph: number;
  runtimeSafety: number;
  contractBoundary: number;
  projectionIntegrity: number;
  governanceFreeze: number;
  deduction: number;
}

export interface SelfAuditItem {
  type: string;
  file: string;
  detail: string;
  risk: "LOW" | "MEDIUM" | "HIGH";
}

export interface DeadArtifact {
  kind: string;
  name: string;
  location: string;
  reason: string;
}

export interface EngineMetricsData {
  filesScanned: number;
  cacheHits: number;
  cacheMisses: number;
  violations: number;
  warnings: number;
  generationTimeMs: number;
}
