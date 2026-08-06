import { safeInvoke } from '../tauri';
import type { LoginRequest, LoginResponse } from '../types';

export async function login(request: LoginRequest): Promise<LoginResponse> {
  return await safeInvoke('login', { request });
}
