<!--
  AppAlert — تنبيه موحد
  الخصائص:
    intent: success | warning | danger | info
    title: string (اختياري)
    dismissible: boolean
  الأحداث: on:dismiss
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let intent: 'success' | 'warning' | 'danger' | 'info' = 'info';
  export let title: string | undefined = undefined;
  export let dismissible = false;

  const dispatch = createEventDispatcher<{ dismiss: void }>();

  const intentMap: Record<string, { wrapper: string; icon: string; iconPath: string }> = {
    success: {
      wrapper: 'bg-green-50 dark:bg-green-900/20 border-green-200 dark:border-green-800 text-green-800 dark:text-green-300',
      icon: 'text-green-500 dark:text-green-400',
      iconPath: 'M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z',
    },
    warning: {
      wrapper: 'bg-yellow-50 dark:bg-yellow-900/20 border-yellow-200 dark:border-yellow-800 text-yellow-800 dark:text-yellow-300',
      icon: 'text-yellow-500 dark:text-yellow-400',
      iconPath: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z',
    },
    danger: {
      wrapper: 'bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800 text-red-800 dark:text-red-300',
      icon: 'text-red-500 dark:text-red-400',
      iconPath: 'M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z',
    },
    info: {
      wrapper: 'bg-blue-50 dark:bg-blue-900/20 border-blue-200 dark:border-blue-800 text-blue-800 dark:text-blue-300',
      icon: 'text-blue-500 dark:text-blue-400',
      iconPath: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
    },
  };

  $: config = intentMap[intent];

  const ariaLiveMap: Record<string, 'polite' | 'assertive'> = {
    success: 'polite',
    warning: 'polite',
    danger: 'assertive',
    info: 'polite',
  };
</script>

<div
  role="alert"
  aria-live={ariaLiveMap[intent]}
  class="flex gap-3 rounded-lg border p-4 {config.wrapper}"
>
  <!-- أيقونة النوع -->
  <svg
    class="h-5 w-5 flex-shrink-0 mt-0.5 {config.icon}"
    fill="none"
    stroke="currentColor"
    viewBox="0 0 24 24"
    aria-hidden="true"
  >
    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d={config.iconPath} />
  </svg>

  <!-- المحتوى -->
  <div class="flex-1 min-w-0">
    {#if title}
      <p class="font-semibold text-sm mb-1">{title}</p>
    {/if}
    <div class="text-sm">
      <slot />
    </div>
  </div>

  <!-- زر الإغلاق -->
  {#if dismissible}
    <button
      type="button"
      class="flex-shrink-0 rounded p-0.5 opacity-70 hover:opacity-100 transition-opacity focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-current"
      aria-label="إغلاق التنبيه"
      on:click={() => dispatch('dismiss')}
    >
      <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  {/if}
</div>
