import { safeInvoke } from '../tauri';
import type { LoginRequest, LoginResponse } from '../types';

export async function login(request: LoginRequest): Promise<LoginResponse> {
  return await safeInvoke('login', { request });
}

export async function changePassword(userId: string, newPassword: string): Promise<boolean> {
  return await safeInvoke('change_password', { target_user_id: userId, new_password: newPassword });
}
