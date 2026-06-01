import { safeInvoke } from '../tauri';
import type { User, SessionStatus, Settings } from '../types';

export async function logout(): Promise<boolean> {
  return await safeInvoke('logout');
}

export async function checkSession(): Promise<SessionStatus> {
  return await safeInvoke('check_session');
}

export async function getCurrentUser(): Promise<User | null> {
  return await safeInvoke<User | null>('get_current_user');
}

export async function touchSession(): Promise<void> {
  return await safeInvoke('touch_session');
}

export async function getSettings(): Promise<Settings> {
  return await safeInvoke('get_settings');
}

export async function configureAsWilaya(wilayaCode: string, wilayaName: string): Promise<Settings> {
  return await safeInvoke('configure_as_wilaya', { wilayaCode, wilayaName });
}

export async function isConfigured(): Promise<boolean> {
  return await safeInvoke('is_configured');
}
