import { safeInvoke } from '../tauri';
import type { LoginResponse } from '../types';

// Identity bootstrap (B5) — RFC 2026-08-04-node-identity-trust §3.6–3.7 / ADR-0038.
// All commands are PRE-AUTH and synchronous; the Root signs fully offline, so
// only operator-facing request/finalize files ever leave the device.

export type IdentityBootstrapState =
  | 'UNINITIALIZED'
  | 'WAITING_FOR_ROOT_CERTIFICATE'
  | 'WILAYA_ACTIVE'
  | 'ADMIN_PROVISIONED'
  | 'READY'
  | 'UNIT_WAITING_FOR_CERTIFICATE'
  | 'UNIT_ACTIVE';

export type IdentitySubjectType = 'WILAYA' | 'UNIT' | 'ADMIN';

export type IdentityCredentialStatus = 'ACTIVE' | 'SUSPENDED' | 'REVOKED' | 'EXPIRED';

export interface IdentityCertificateDto {
  identity_id: string;
  subject_type: IdentitySubjectType;
  subject_id: string;
  issuer_identity_id: string | null;
  credential_id: string;
  generation: number;
  status: IdentityCredentialStatus;
  public_key: number[];
  algorithm_version: number;
  not_after: string | null;
  package_sequence: number | null;
  signature: string | null;
}

export type FinalizeWilayaProvisionResultDto =
  | { Provisioned: IdentityCertificateDto }
  | { AlreadyProvisioned: IdentityCertificateDto };

export type FinalizeUnitProvisionResultDto =
  | { Provisioned: IdentityCertificateDto }
  | { AlreadyProvisioned: IdentityCertificateDto };

export type InstallWilayaCertificateResultDto =
  | { Installed: IdentityCertificateDto }
  | { AlreadyInstalled: IdentityCertificateDto };

// Credential rotation (B7) — RFC 2026-08-04-node-identity-trust §3.3 / §3.12 / ADR-0038.
// All rotation commands are POST-AUTH and synchronous.

export type RotationOperation = 'ROTATE' | 'RE-ISSUE';

export interface RotationPlanDto {
  operation: RotationOperation;
  previous_credential_id: string;
  certificate: IdentityCertificateDto;
}

export type RotationFinalizeOutcomeDto =
  | { Completed: { certificate: IdentityCertificateDto; operation: RotationOperation } }
  | { AlreadyCompleted: { certificate: IdentityCertificateDto } };

export interface SignedUnitRotationDto {
  certificate: IdentityCertificateDto;
  operation: RotationOperation;
}

export interface ChallengeMessageDto {
  protocol_version: number;
  session_id: string;
  node_identity_id: string;
  nonce: number[];
  challenge_version: number;
}

/** Derived bootstrap state of the local node (pure projection, never stored). */
export async function getIdentityStatus(): Promise<IdentityBootstrapState> {
  return await safeInvoke('get_identity_status');
}

/**
 * Begin offline WILAYA bootstrap: generate the node keypair, persist the node
 * secret, and write the UNSIGNED certificate (CSR) to `requestFile` for the
 * operator to carry to the Authority Root.
 */
export async function beginWilayaProvision(requestFile: string): Promise<IdentityCertificateDto> {
  return await safeInvoke('begin_wilaya_provision', { requestFilePath: requestFile });
}

/**
 * Finalize offline WILAYA bootstrap with the Root-signed certificate read from
 * `certFile`. Idempotent: an identical re-presentation is a no-op.
 */
export async function finalizeWilayaProvision(
  certFile: string,
): Promise<FinalizeWilayaProvisionResultDto> {
  return await safeInvoke('finalize_wilaya_provision', { certFilePath: certFile });
}

/**
 * Issue the FIRST ADMIN key bound to `subjectUsername`, protecting the portable
 * `.adminkey` with `passphrase`. Links the `users` row additively.
 */
export async function issueFirstAdminKey(
  subjectUsername: string,
  passphrase: string,
): Promise<IdentityCertificateDto> {
  return await safeInvoke('issue_first_admin_key', {
    subjectUsername,
    passphrase,
  });
}

/**
 * Begin UNIT bootstrap (RFC §3.12, B6-A): resolve the LOCAL `subject_id` (the
 * node's own `units` row), generate the UNIT keypair, persist the node secret,
 * and write the UNSIGNED UNIT certificate (CSR) to `requestFile`.
 */
export async function beginUnitProvision(requestFile: string): Promise<IdentityCertificateDto> {
  return await safeInvoke('begin_unit_provision', { requestFilePath: requestFile });
}

/**
 * WILAYA side: sign a UNIT CSR. The CSR's `subject_id` is validated against the
 * local `units` table (never overridden); the ACTIVE local WILAYA signs it.
 * Pass the CSR payload JSON (not a file path).
 */
export async function signUnitIdentityRequest(requestJson: string): Promise<IdentityCertificateDto> {
  return await safeInvoke('sign_unit_identity_request', { requestJson });
}

/**
 * Finalize UNIT bootstrap with the WILAYA-signed certificate read from
 * `certFile`. The issuer is resolved via `issuer_identity_id` and verified
 * (exists + ACTIVE + WILAYA). Idempotent.
 */
export async function finalizeUnitProvision(
  certFile: string,
): Promise<FinalizeUnitProvisionResultDto> {
  return await safeInvoke('finalize_unit_provision', { certFilePath: certFile });
}

/**
 * Install (or idempotently re-confirm) the ACTIVE WILAYA certificate as the
 * UNIT's LOCAL TRUST ANCHOR (strict two-step flow, RFC §3.12). Reads the
 * Root-signed WILAYA certificate from `certFile`.
 */
export async function installWilayaCertificate(
  certFile: string,
): Promise<InstallWilayaCertificateResultDto> {
  return await safeInvoke('install_wilaya_certificate', { certFilePath: certFile });
}

/**
 * Begin a WILAYA credential rotation (B7): stage a fresh node key (the ACTIVE
 * key is untouched until finalize) and write the UNSIGNED rotation CSR to
 * `requestFile` for the operator to carry to the Authority Root.
 */
export async function beginWilayaRotation(
  requestFile: string,
  operation: RotationOperation,
): Promise<RotationPlanDto> {
  return await safeInvoke('begin_wilaya_rotation', {
    requestFilePath: requestFile,
    operation,
  });
}

/**
 * Finalize the WILAYA rotation with the Root-signed certificate read from
 * `certFile`. The rotation Trust Package (signed with the OLD node key) is
 * written to `packageFile` BEFORE the new secret is promoted; on any later
 * failure the old key is restored (no partial state).
 */
export async function finalizeWilayaRotation(
  certFile: string,
  packageFile: string,
): Promise<RotationFinalizeOutcomeDto> {
  return await safeInvoke('finalize_wilaya_rotation', {
    certFilePath: certFile,
    rotationPackagePath: packageFile,
  });
}

/**
 * Begin a UNIT credential rotation on the UNIT node itself (B7): stage a fresh
 * node key and write the UNSIGNED rotation CSR to `requestFile` for the
 * operator to carry to the WILAYA node.
 */
export async function beginUnitRotation(
  requestFile: string,
  operation: RotationOperation,
): Promise<RotationPlanDto> {
  return await safeInvoke('begin_unit_rotation', {
    requestFilePath: requestFile,
    operation,
  });
}

/**
 * WILAYA side: sign a UNIT rotation CSR. The CSR's `subject_id` is validated
 * against the local `units` table; the ACTIVE local WILAYA signs it. The signed
 * certificate is also recorded as WILAYA-side Issuer Local State (best-effort
 * credential guard). Pass the CSR payload JSON (not a file path).
 */
export async function signUnitRotationRequest(requestJson: string): Promise<SignedUnitRotationDto> {
  return await safeInvoke('sign_unit_rotation_request', { requestJson });
}

/**
 * Finalize the UNIT rotation with the WILAYA-signed certificate read from
 * `certFile`. The issuer is resolved via `issuer_identity_id` and verified
 * (exists + ACTIVE + WILAYA). Fail-closed; idempotent.
 */
export async function finalizeUnitRotation(
  certFile: string,
): Promise<RotationFinalizeOutcomeDto> {
  return await safeInvoke('finalize_unit_rotation', { certFilePath: certFile });
}

/** Begin a one-shot Challenge–Response login. */
export async function beginChallenge(): Promise<ChallengeMessageDto> {
  return await safeInvoke('begin_challenge');
}

/**
 * Complete a Challenge–Response login with the `.adminkey` passphrase. The
 * backend decrypts the key and signs the challenge in Rust — the frontend only
 * ever passes the passphrase.
 */
export async function completeChallenge(sessionId: string, passphrase: string): Promise<LoginResponse> {
  return await safeInvoke('complete_challenge', { sessionId, passphrase });
}
