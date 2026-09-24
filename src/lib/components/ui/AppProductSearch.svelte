<!--
  AppProductSearch — حقل بحث منتجات مضغوط (نفس نمط "مخزون الوحدات")
  الخصائص:
    search: string (ربط ثنائي)
    placeholder: string
    id: string (اختياري)
-->
<script lang="ts">
  let {
    search = $bindable(''),
    placeholder = 'بحث عن منتج...',
    id = undefined,
  }: {
    search?: string;
    placeholder?: string;
    id?: string;
  } = $props();
</script>

<div class="flex items-center gap-2">
  <input
    {id}
    type="text"
    bind:value={search}
    {placeholder}
    class="px-3 py-1.5 border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 rounded-lg text-sm w-48 transition-colors"
  />
  <button
    type="button"
    class="shrink-0 inline-flex items-center justify-center w-7 h-7 rounded-lg text-base font-medium text-gray-500 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-gray-600 border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 transition-colors disabled:opacity-35 disabled:cursor-not-allowed"
    disabled={!search}
    aria-label="مسح البحث"
    title="مسح البحث"
    onclick={() => (search = '')}
  >×</button>
</div>