<!--
  AppBadge — شارة الحالة
  الخصائص:
    intent: success | warning | danger | info | neutral
    size: sm | md
-->
<script lang="ts">
  export let intent: 'success' | 'warning' | 'danger' | 'info' | 'neutral' = 'neutral';
  export let size: 'sm' | 'md' = 'sm';

  const intentMap: Record<string, string> = {
    success: 'bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-400 border-green-200 dark:border-green-800',
    warning: 'bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-400 border-yellow-200 dark:border-yellow-800',
    danger:  'bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-400 border-red-200 dark:border-red-800',
    info:    'bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-400 border-blue-200 dark:border-blue-800',
    neutral: 'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 border-gray-200 dark:border-gray-600',
  };

  const sizeMap: Record<string, string> = {
    sm: 'px-2 py-0.5 text-xs',
    md: 'px-2.5 py-1 text-sm',
  };

  $: classes = [
    'inline-flex items-center font-medium rounded-full border',
    intentMap[intent],
    sizeMap[size],
  ].join(' ');
</script>

<span class={classes}>
  <slot />
</span>
