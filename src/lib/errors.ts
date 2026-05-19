/**
 * Unified error normalization and formatting for the frontend.
 * Ensures that all backend exceptions are sanitized and user-friendly.
 */

export interface NormalizedError {
  message: string;
  originalError: unknown;
  isNetworkError: boolean;
  isBackendError: boolean;
}

export function normalizeError(error: unknown): NormalizedError {
  let message = 'حدث خطأ غير متوقع.';
  let isNetworkError = false;
  let isBackendError = false;

  if (typeof error === 'string') {
    isBackendError = true;
    message = extractBackendMessage(error);
  } else if (error instanceof Error) {
    if (error.message.includes('fetch') || error.message.includes('network')) {
      isNetworkError = true;
      message = 'مشكلة في الاتصال بالشبكة.';
    } else {
      isBackendError = true;
      message = extractBackendMessage(error.message);
    }
  } else if (error && typeof error === 'object' && 'message' in error) {
    isBackendError = true;
    message = extractBackendMessage(String(error.message));
  }

  return {
    message,
    originalError: error,
    isNetworkError,
    isBackendError,
  };
}

export function extractBackendMessage(rawMessage: string): string {
  // Try to remove generic "Error: " prefixes
  let cleaned = rawMessage.replace(/^Error:\s*/i, '');
  
  // Handle some common backend errors explicitly if needed
  if (cleaned.includes('IO error') || cleaned.includes('No such file')) {
    return 'خطأ في قراءة أو كتابة الملفات.';
  }
  if (cleaned.includes('database is locked')) {
    return 'قاعدة البيانات قيد الاستخدام حالياً.';
  }
  if (cleaned.includes('unauthorized') || cleaned.includes('Unauthorized')) {
    return 'غير مصرح بالدخول.';
  }
  
  return cleaned;
}

export function formatErrorMessage(error: unknown): string {
  return normalizeError(error).message;
}
