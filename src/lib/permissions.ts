import type { User, Settings } from './types';

// Role-based action definitions
export const ROLE_PERMISSIONS: Record<'Admin' | 'User', string[]> = {
  Admin: [
    'view_observability',
    'view_audit_log',
    'verify_audit_integrity',
    'view_system_health',
    'advanced_diagnostics',
    'view_sync_topology',
    'resolve_conflicts',
    'manage_fiscal',
  ],
  User: [
    'view_dashboard',
    'manage_stock',
    'manage_orders',
    'manage_consumption',
    'view_reports',
  ],
};

export function hasPermission(
  user: User | null,
  action: string,
): boolean {
  if (!user) return false;
  
  // Admin automatically has all User permissions too
  if (user.role === 'Admin') {
    return true;
  }
  
  const permissions = ROLE_PERMISSIONS[user.role];
  return permissions ? permissions.includes(action) : false;
}

export function isRouteAllowed(
  user: User | null,
  settings: Settings | null,
  path: string,
): boolean {
  if (path === '/' || path === '/login' || path === '/configure') {
    return true;
  }

  if (!user || !settings) return false;

  // Admin observability path guards
  if (path.startsWith('/admin') || path === '/audit-log') {
    return user.role === 'Admin';
  }

  // Wilaya-specific routes
  if (path.startsWith('/wilaya')) {
    return settings.node_type === 'WILAYA';
  }

  // Unit-specific routes
  if (path.startsWith('/unit')) {
    return settings.node_type === 'UNIT';
  }

  return true;
}

export function shouldRedirectToLogin(
  isAuthenticated: boolean,
  path: string,
): boolean {
  if (path === '/' || path === '/login' || path === '/configure') {
    return false;
  }
  return !isAuthenticated;
}
