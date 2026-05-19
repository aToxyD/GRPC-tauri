<!--
  AppEmptyState — حالة فراغ موحدة
  الخصائص:
    title: string
    description: string (اختياري)
    icon: string (مسار SVG path اختياري)
  الفتحات: action (إجراء اختياري)
-->
<script lang="ts">
  interface $$Slots {
    default: {};
    action: {};
  }
  export let title: string;
  export let description: string | undefined = undefined;
  export let icon: string | undefined = undefined;

  const defaultIconPath = 'M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4';
</script>

<div class="flex flex-col items-center justify-center py-16 gap-4 text-center">
  <div class="h-16 w-16 rounded-full bg-gray-100 dark:bg-gray-700 flex items-center justify-center" aria-hidden="true">
    <svg class="h-8 w-8 text-gray-400 dark:text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d={icon ?? defaultIconPath} />
    </svg>
  </div>
  <div class="max-w-sm">
    <p class="font-semibold text-gray-700 dark:text-gray-300">{title}</p>
    {#if description}
      <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{description}</p>
    {/if}
  </div>
  {#if $$slots.action}
    <slot name="action" />
  {/if}
</div>
