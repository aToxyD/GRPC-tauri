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
  export let title: string | undefined = undefined;
  export let description: string | undefined = undefined;
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
  <slot />
</section>
