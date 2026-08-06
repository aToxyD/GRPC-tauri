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
  | 'READY';

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
  return await safeInvoke('begin_wilaya_provision', { request_file_path: requestFile });
}

/**
 * Finalize offline WILAYA bootstrap with the Root-signed certificate read from
 * `certFile`. Idempotent: an identical re-presentation is a no-op.
 */
export async function finalizeWilayaProvision(
  certFile: string,
): Promise<FinalizeWilayaProvisionResultDto> {
  return await safeInvoke('finalize_wilaya_provision', { cert_file_path: certFile });
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
    subject_username: subjectUsername,
    passphrase,
  });
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
  return await safeInvoke('complete_challenge', { session_id: sessionId, passphrase });
}
