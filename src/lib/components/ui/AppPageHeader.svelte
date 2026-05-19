<!--
  AppPageHeader — رأس الصفحة الموحد
  الخصائص:
    title: string
    subtitle: string (اختياري)
  الفتحات: actions (أزرار الإجراءات اليمنى)
-->
<script lang="ts">
  export let title: string;
  export let subtitle: string | undefined = undefined;
</script>

<div class="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4 mb-6">
  <div>
    <h1 class="text-2xl font-bold text-gray-900 dark:text-white">{title}</h1>
    {#if subtitle}
      <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{subtitle}</p>
    {/if}
  </div>
  {#if $$slots.actions}
    <div class="flex items-center gap-2 flex-shrink-0">
      <slot name="actions" />
    </div>
  {/if}
</div>
