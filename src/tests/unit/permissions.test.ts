import { describe, it, expect } from 'vitest';
import { hasPermission, isRouteAllowed, shouldRedirectToLogin } from '../../lib/permissions';
import type { User, Settings } from '../../lib/types';

describe('Permissions & Routing Helpers', () => {
  const adminUser: User = {
    id: '1',
    username: 'admin',
    role: 'Admin',
    created_at: '',
  };

  const regularUser: User = {
    id: '2',
    username: 'user',
    role: 'User',
    created_at: '',
  };

  const wilayaSettings: Settings = {
    id: 1,
    node_type: 'WILAYA',
    unit_name: null,
    unit_code: null,
    current_year: 2026,
    wilaya_code: '16',
    wilaya_name: 'Alger',
    configured: true,
  };

  const unitSettings: Settings = {
    id: 2,
    node_type: 'UNIT',
    unit_name: 'Unit A',
    unit_code: 'UA',
    current_year: 2026,
    wilaya_code: '16',
    wilaya_name: 'Alger',
    configured: true,
  };

  describe('hasPermission', () => {
    it('should reject permissions for unauthenticated users', () => {
      expect(hasPermission(null, 'view_dashboard')).toBe(false);
    });

    it('should grant all permissions to Admin', () => {
      expect(hasPermission(adminUser, 'advanced_diagnostics')).toBe(true);
      expect(hasPermission(adminUser, 'view_dashboard')).toBe(true);
    });

    it('should filter permissions for regular Users', () => {
      expect(hasPermission(regularUser, 'view_dashboard')).toBe(true);
      expect(hasPermission(regularUser, 'advanced_diagnostics')).toBe(false);
    });
  });

  describe('isRouteAllowed', () => {
    it('should allow public routes', () => {
      expect(isRouteAllowed(null, null, '/login')).toBe(true);
      expect(isRouteAllowed(null, null, '/configure')).toBe(true);
    });

    it('should restrict admin observability routes to Admin role only', () => {
      expect(isRouteAllowed(regularUser, wilayaSettings, '/admin/system-health')).toBe(false);
      expect(isRouteAllowed(adminUser, wilayaSettings, '/admin/system-health')).toBe(true);
    });

    it('should restrict Wilaya routes to Wilaya node settings only', () => {
      expect(isRouteAllowed(regularUser, unitSettings, '/wilaya/products')).toBe(false);
      expect(isRouteAllowed(regularUser, wilayaSettings, '/wilaya/products')).toBe(true);
    });

    it('should restrict Unit routes to Unit node settings only', () => {
      expect(isRouteAllowed(regularUser, wilayaSettings, '/unit/stock')).toBe(false);
      expect(isRouteAllowed(regularUser, unitSettings, '/unit/stock')).toBe(true);
    });
  });

  describe('shouldRedirectToLogin', () => {
    it('should not redirect for public routes', () => {
      expect(shouldRedirectToLogin(false, '/login')).toBe(false);
      expect(shouldRedirectToLogin(false, '/configure')).toBe(false);
    });

    it('should redirect for private routes when not authenticated', () => {
      expect(shouldRedirectToLogin(false, '/wilaya/dashboard')).toBe(true);
      expect(shouldRedirectToLogin(true, '/wilaya/dashboard')).toBe(false);
    });
  });
});
