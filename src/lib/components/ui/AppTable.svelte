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
  import AppLoadingState from './AppLoadingState.svelte';
  import AppEmptyState from './AppEmptyState.svelte';
  import AppAlert from './AppAlert.svelte';

  interface $$Slots {
    head: {};
    default: {};
    empty: {};
    'empty-action': {};
  }
  export let loading = false;
  export let empty = false;
  export let emptyMessage = 'لا توجد بيانات';
  export let error: string | null | undefined = undefined;
  export let caption: string | undefined = undefined;
</script>

<div class="w-full overflow-x-auto rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 shadow-sm">
  {#if error}
    <div class="p-4" aria-live="assertive">
      <AppAlert intent="danger">{error}</AppAlert>
    </div>
  {:else if loading}
    <AppLoadingState message="جارٍ التحميل..." />
  {:else}
    {#if empty}
      {#if $$slots.empty}
        <slot name="empty" />
      {:else}
        <AppEmptyState title={emptyMessage}>
          <slot name="empty-action" slot="action" />
        </AppEmptyState>
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
  {/if}
</div>
