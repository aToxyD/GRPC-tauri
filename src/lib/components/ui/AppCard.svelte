<!--
  AppCard — حاوية محتوى موحدة
  الخصائص:
    elevated: boolean — ظل مرتفع
    padding: none | sm | md | lg
    noBorder: boolean — بدون حد
-->
<script lang="ts">
  import AppLoadingState from './AppLoadingState.svelte';
  import AppEmptyState from './AppEmptyState.svelte';
  import AppAlert from './AppAlert.svelte';

  export let elevated = false;
  export let padding: 'none' | 'sm' | 'md' | 'lg' = 'md';
  export let noBorder = false;
  export let loading = false;
  export let empty = false;
  export let emptyMessage = 'لا توجد بيانات';
  export let error: string | null | undefined = undefined;

  let className = '';
  export { className as class };

  const paddingMap: Record<string, string> = {
    none: '',
    sm: 'p-4',
    md: 'p-6',
    lg: 'p-8',
  };

  $: classes = [
    'bg-white dark:bg-gray-800 rounded-xl transition-colors duration-200',
    elevated
      ? 'shadow-lg dark:shadow-gray-950/50'
      : 'shadow-sm dark:shadow-gray-950/30',
    noBorder ? '' : 'border border-gray-200 dark:border-gray-700',
    paddingMap[padding],
    className,
  ]
    .filter(Boolean)
    .join(' ');
</script>

<div class={classes}>
  {#if error}
    <div aria-live="assertive">
      <AppAlert intent="danger">{error}</AppAlert>
    </div>
  {:else if loading}
    <AppLoadingState message="جارٍ التحميل..." />
  {:else if empty}
    {#if $$slots.empty}
      <slot name="empty" />
    {:else}
      <AppEmptyState title={emptyMessage}>
        <slot name="empty-action" slot="action" />
      </AppEmptyState>
    {/if}
  {:else}
    <slot />
  {/if}
</div>
