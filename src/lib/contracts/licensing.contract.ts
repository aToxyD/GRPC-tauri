import { safeInvoke } from '../tauri';

// Licensing (ADR-0042).
// The backend owns ALL licensing decisions: artifact verification (structure /
// version / identity / canonicality / key binding / signature), the type
// registry, entitlement mapping, subject binding, and the enforcement gate.
// The frontend only renders the derived projections below. License and anchor
// packages are carried as canonical JSON strings (pasted or loaded by the
// operator); they are never reconstructed on this side.

export interface TrustAnchorViewDto {
  installed: boolean;
  key_id: string | null;
}

export interface LicenseViewDto {
  license_id: string;
  artifact_id: string;
  type_key: string;
  subject_id: string;
  entitlements: string[];
  status: string;
  contract_version_major: number;
  contract_version_minor: number;
  imported_at: string;
  last_verified_at: string | null;
  /** Active + re-verifies valid (full pipeline) + node-bound. */
  enforceable: boolean;
}

export interface LicensingSummaryDto {
  anchor_installed: boolean;
  license_count: number;
  active_license_count: number;
  /** Dormant until an active anchor is installed (ADR-0042 §5). */
  gate_active: boolean;
}

export interface LicensingStatusDto {
  anchor: TrustAnchorViewDto;
  licenses: LicenseViewDto[];
  summary: LicensingSummaryDto;
}

export interface ImportAnchorResultDto {
  installed: boolean;
  key_id: string;
  /** The previous active anchor's key_id, when this import rotated it. */
  replaced_key_id: string | null;
}

export type LicenseImportOutcome = 'imported' | 'rejected' | 'not-for-this-node';

export interface ImportLicenseResultDto {
  outcome: LicenseImportOutcome;
  license_id: string | null;
  artifact_id: string | null;
  message: string;
}

export interface VerifyLicenseReportDto {
  checked: number;
  enforceable: number;
  results: LicenseViewDto[];
}

/** Read-only live licensing projection (anchor + licenses + gate state). */
export async function getLicensingStatus(): Promise<LicensingStatusDto> {
  return await safeInvoke<LicensingStatusDto>('get_licensing_status');
}

/** Install or rotate the single active licensing trust anchor (provisioning-v1). */
export async function importTrustAnchor(packageJson: string): Promise<ImportAnchorResultDto> {
  return await safeInvoke<ImportAnchorResultDto>('import_trust_anchor', { package_json: packageJson });
}

/** Import a signed license artifact (full pipeline + derived-view persist). */
export async function importLicense(artifactJson: string): Promise<ImportLicenseResultDto> {
  return await safeInvoke<ImportLicenseResultDto>('import_license', { artifact_json: artifactJson });
}

/** Deterministic pre-import check (no persistence). */
export async function dryRunVerifyLicense(artifactJson: string): Promise<ImportLicenseResultDto> {
  return await safeInvoke<ImportLicenseResultDto>('dry_run_verify_license', {
    artifact_json: artifactJson,
  });
}

/** Deterministic re-verification of every stored license. */
export async function verifyLicense(): Promise<VerifyLicenseReportDto> {
  return await safeInvoke<VerifyLicenseReportDto>('verify_license');
}
