<!--
  AppSection — قسم محتوى موحد
  الخصائص:
    title: string (اختياري)
    description: string (اختياري)
  الفتحات:
    default: المحتوى
    actions: إجراءات رأس القسم
-->
<script lang="ts">
  import AppLoadingState from './AppLoadingState.svelte';
  import AppEmptyState from './AppEmptyState.svelte';
  import AppAlert from './AppAlert.svelte';

  export let title: string | undefined = undefined;
  export let description: string | undefined = undefined;
  export let loading = false;
  export let empty = false;
  export let emptyMessage = 'لا توجد بيانات';
  export let error: string | null | undefined = undefined;
</script>

<section class="mb-6">
  {#if title || $$slots.actions}
    <div class="flex items-center justify-between mb-4">
      {#if title}
        <div>
          <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100">{title}</h2>
          {#if description}
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5">{description}</p>
          {/if}
        </div>
      {/if}
      {#if $$slots.actions}
        <div class="flex items-center gap-2">
          <slot name="actions" />
        </div>
      {/if}
    </div>
  {/if}

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
</section>
