/**
 * Deterministic UI safety helpers for critical operations.
 * Pure functions — no Tauri dependency (lightweight unit tests).
 */

export type GuardedOperationKey =
  | 'fiscal_close'
  | 'archive'
  | 'restore'
  | 'import_historical';

export interface CriticalOperationUiState {
  confirmation: string;
  executionToken: string;
  issuedToken: string | null;
  isSubmitting: boolean;
}

export const CONFIRMATION_TEXT: Record<GuardedOperationKey, string> = {
  fiscal_close: '', // dynamic: current fiscal year as string
  archive: 'ARCHIVE',
  restore: 'RESTORE',
  import_historical: 'IMPORT-HISTORICAL',
};

export function isConfirmationValid(
  operation: GuardedOperationKey,
  confirmation: string,
  dynamicYear?: number,
): boolean {
  const trimmed = confirmation.trim();
  switch (operation) {
    case 'fiscal_close':
      return dynamicYear !== undefined && trimmed === String(dynamicYear);
    case 'archive':
      return trimmed === CONFIRMATION_TEXT.archive;
    case 'restore':
      return trimmed === CONFIRMATION_TEXT.restore;
    case 'import_historical':
      return trimmed === CONFIRMATION_TEXT.import_historical;
    default:
      return false;
  }
}

export function isExecutionTokenValid(
  state: CriticalOperationUiState,
): boolean {
  const token = state.executionToken.trim();
  if (!token || !state.issuedToken) return false;
  return token === state.issuedToken.trim();
}

export function isCriticalActionEnabled(
  operation: GuardedOperationKey,
  state: CriticalOperationUiState,
  dynamicYear?: number,
): boolean {
  if (state.isSubmitting) return false;
  if (!isConfirmationValid(operation, state.confirmation, dynamicYear)) return false;
  if (!isExecutionTokenValid(state)) return false;
  return true;
}

export function isStaleTokenRejected(
  state: CriticalOperationUiState,
): boolean {
  const token = state.executionToken.trim();
  if (!token || !state.issuedToken) return false;
  return token !== state.issuedToken.trim();
}

export function initialCriticalUiState(): CriticalOperationUiState {
  return {
    confirmation: '',
    executionToken: '',
    issuedToken: null,
    isSubmitting: false,
  };
}
