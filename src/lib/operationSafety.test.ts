import { describe, expect, it } from 'bun:test';
import {
  initialCriticalUiState,
  isConfirmationValid,
  isCriticalActionEnabled,
  isExecutionTokenValid,
  isStaleTokenRejected,
} from './operationSafety';

describe('operationSafety', () => {
  it('critical action starts disabled', () => {
    const state = initialCriticalUiState();
    expect(isCriticalActionEnabled('restore', state)).toBe(false);
    expect(isCriticalActionEnabled('archive', state)).toBe(false);
  });

  it('requires confirmation text', () => {
    const state = {
      ...initialCriticalUiState(),
      executionToken: 'tok',
      issuedToken: 'tok',
    };
    expect(isConfirmationValid('restore', 'RESTORE')).toBe(true);
    expect(isConfirmationValid('restore', 'restore')).toBe(false);
    expect(isCriticalActionEnabled('restore', { ...state, confirmation: 'RESTORE' })).toBe(
      true,
    );
  });

  it('requires execution token match', () => {
    const state = {
      ...initialCriticalUiState(),
      confirmation: 'ARCHIVE',
      executionToken: '',
      issuedToken: 'abc123',
    };
    expect(isExecutionTokenValid(state)).toBe(false);
    expect(
      isCriticalActionEnabled('archive', { ...state, executionToken: 'abc123' }),
    ).toBe(true);
  });

  it('rejects stale token visually', () => {
    const state = {
      ...initialCriticalUiState(),
      confirmation: 'RESTORE',
      executionToken: 'old-token',
      issuedToken: 'fresh-token',
    };
    expect(isStaleTokenRejected(state)).toBe(true);
    expect(isCriticalActionEnabled('restore', state)).toBe(false);
  });

  it('fiscal close uses dynamic year confirmation', () => {
    expect(isConfirmationValid('fiscal_close', '2025', 2025)).toBe(true);
    expect(isConfirmationValid('fiscal_close', '2024', 2025)).toBe(false);
  });
});
