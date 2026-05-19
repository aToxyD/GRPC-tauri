<!--
  AppTable — جدول بيانات موحد
  الخصائص:
    loading: boolean — حالة التحميل
    empty: boolean — حالة الفراغ
    emptyMessage: string — رسالة الفراغ
    caption: string — وصف الجدول لإمكانية الوصول
  الفتحات:
    head: عناوين الأعمدة (عناصر th)
    default: صفوف البيانات (عناصر tr)
    empty-action: إجراء اختياري في حالة الفراغ
-->
<script lang="ts">
  interface $$Slots {
    head: {};
    default: {};
    empty: {};
    'empty-action': {};
  }
  export let loading = false;
  export let empty = false;
  export let emptyMessage = 'لا توجد بيانات';
  export let caption: string | undefined = undefined;
</script>

<div class="w-full overflow-x-auto rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 shadow-sm">
  {#if loading}
    <!-- حالة التحميل -->
    <div class="flex items-center justify-center py-16 text-gray-400 dark:text-gray-500" aria-busy="true" aria-label="جارٍ التحميل">
      <svg class="animate-spin h-6 w-6 ml-2" fill="none" viewBox="0 0 24 24" aria-hidden="true">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
      </svg>
      <span class="text-sm">جارٍ التحميل...</span>
    </div>
  {:else if empty}
    <!-- حالة الفراغ -->
    {#if $$slots.empty}
      <slot name="empty" />
    {:else}
      <div class="flex flex-col items-center justify-center py-16 gap-3 text-gray-400 dark:text-gray-500">
        <svg class="h-10 w-10 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
        </svg>
        <p class="text-sm font-medium">{emptyMessage}</p>
        {#if $$slots['empty-action']}
          <slot name="empty-action" />
        {/if}
      </div>
    {/if}
  {:else}
    <table class="w-full border-collapse">
      {#if caption}
        <caption class="sr-only">{caption}</caption>
      {/if}
      <thead>
        <tr>
          <slot name="head" />
        </tr>
      </thead>
      <tbody>
        <slot />
      </tbody>
    </table>
  {/if}
</div>
