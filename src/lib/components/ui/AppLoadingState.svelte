<!--
  AppLoadingState — حالة تحميل الصفحة الكاملة
  الخصائص:
    message: string — رسالة التحميل
-->
<script lang="ts">
  export let message = 'جارٍ التحميل...';
</script>

<div
  class="flex flex-col items-center justify-center py-20 gap-4 text-gray-400 dark:text-gray-500"
  aria-busy="true"
  aria-live="polite"
  role="status"
>
  <svg
    class="animate-spin h-8 w-8 text-blue-500 dark:text-blue-400"
    fill="none"
    viewBox="0 0 24 24"
    aria-hidden="true"
  >
    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
    <path
      class="opacity-75"
      fill="currentColor"
      d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
    />
  </svg>
  <p class="text-sm font-medium">{message}</p>
</div>
