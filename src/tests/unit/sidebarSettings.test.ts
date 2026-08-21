/**
 * SEC-014 Phase 4 — Sidebar navigation entry for SettingsPage.
 *
 * UX visibility only — authorization stays backend-authoritative.
 * Visible: WILAYA Admin, UNIT Admin, UNIT User. Hidden: WILAYA User.
 * Placement: after "النسخ الاحتياطية", before the admin observability block.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import Sidebar from '../../components/Sidebar.svelte';

let currentUserValue: { username: string; role: string } | null = null;

const mockGetSettings = vi.fn();
const mockCurrentUserSubscribe = vi.fn((listener: (value: unknown) => void) => {
  listener(currentUserValue);
  return () => {};
});
const mockThemeSubscribe = vi.fn((listener: (value: unknown) => void) => {
  listener('dark');
  return () => {};
});

vi.mock('../../lib/session', () => ({
  currentUser: { subscribe: (l: any) => mockCurrentUserSubscribe(l) },
  logout: vi.fn(),
}));

vi.mock('../../lib/theme', () => ({
  theme: { subscribe: (l: any) => mockThemeSubscribe(l) },
  toggleTheme: vi.fn(),
}));

vi.mock('lucide-svelte', () => {
  const Stub = function () {};
  return { Sun: Stub, Moon: Stub };
});

vi.mock('../../lib/contracts', () => ({
  getSettings: (...args: any[]) => mockGetSettings(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ maximize: vi.fn() }),
}));

const mockPush = vi.fn();
vi.mock('svelte-spa-router', () => ({
  push: (...args: any[]) => mockPush(...args),
  link: vi.fn(),
}));

const wilayaAdmin = { username: 'admin', role: 'Admin' };
const wilayaUser = { username: 'wilaya-operator', role: 'User' };
const unitAdmin = { username: 'unit-admin', role: 'Admin' };
const unitUser = { username: 'unit-operator', role: 'User' };

function navHrefs(): string[] {
  return Array.from(document.querySelectorAll('nav a'))
    .map((a) => a.getAttribute('href'))
    .filter((h): h is string => h !== null);
}

describe('Sidebar — settings navigation (SEC-014 Phase 4)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
  });

  it('shows the settings link for a WILAYA Admin between backup and observability', async () => {
    currentUserValue = wilayaAdmin;
    render(Sidebar, { nodeType: 'WILAYA' });

    const link = await screen.findByRole('link', { name: /الإعدادات/ });
    expect(link).toHaveAttribute('href', '/settings');

    const hrefs = navHrefs();
    expect(hrefs).toContain('/backup');
    expect(hrefs.indexOf('/backup')).toBeLessThan(hrefs.indexOf('/settings'));
    expect(hrefs.indexOf('/settings')).toBeLessThan(hrefs.indexOf('/audit-log'));
  });

  it('hides the settings link from a WILAYA User', async () => {
    currentUserValue = wilayaUser;
    render(Sidebar, { nodeType: 'WILAYA' });

    await waitFor(() => {
      expect(screen.getByRole('link', { name: /نظرة عامة/ })).toBeInTheDocument();
    });
    expect(screen.queryByRole('link', { name: /الإعدادات/ })).toBeNull();
    expect(navHrefs()).not.toContain('/settings');
  });

  it('shows the settings link for a UNIT Admin', async () => {
    currentUserValue = unitAdmin;
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'UNIT',
      unit_code: 'U1',
    });
    render(Sidebar, { nodeType: 'UNIT' });

    await waitFor(() => {
      expect(screen.getByRole('link', { name: /الإعدادات/ })).toBeInTheDocument();
    });
    expect(navHrefs()).toContain('/settings');
  });

  it('shows the settings link for a UNIT User', async () => {
    currentUserValue = unitUser;
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'UNIT',
      unit_code: 'U1',
    });
    render(Sidebar, { nodeType: 'UNIT' });

    await waitFor(() => {
      expect(screen.getByRole('link', { name: /الإعدادات/ })).toBeInTheDocument();
    });
    expect(navHrefs()).toContain('/settings');
  });
});
