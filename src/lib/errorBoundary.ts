import { writable } from 'svelte/store';
import { telemetry } from './telemetry';
import { showError } from './notifications';

// @category UiState — fatal error display
export const hasFatalError = writable(false);
// @category UiState — fatal error message
export const fatalErrorMessage = writable('');

// @category UiState — singleton initialization guard
let initialized = false;

export function initErrorBoundary() {
  if (initialized) return;
  initialized = true;

  window.onerror = function (message, source, lineno, colno, error) {
    const errorMsg = error?.message || String(message);
    telemetry.trackError(error || new Error(errorMsg), `window.onerror: ${source}:${lineno}:${colno}`);
    telemetry.trackUnhandledRuntime('window.onerror', errorMsg);

    const arabicMessage =
      'حدث خطأ فادح في النظام. تم إغلاق الواجهة تلقائياً لحماية سلامة البيانات.';
    fatalErrorMessage.set(arabicMessage);
    hasFatalError.set(true);

    showError(arabicMessage, 'خطأ فادح في النظام');

    return true;
  };

  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason;
    const errorMsg = reason instanceof Error ? reason.message : String(reason);
    telemetry.trackError(reason, 'unhandledrejection');
    telemetry.trackAsyncFailure('unhandled_promise_rejection', errorMsg);
    telemetry.trackUnhandledRuntime('unhandledrejection', errorMsg);

    const arabicMessage =
      'حدث خطأ في عملية غير متزامنة. تم تسجيل الخطأ للتأكد من سلامة النظام.';
    showError(arabicMessage, 'فشل العملية الخلفية');
  });
}
