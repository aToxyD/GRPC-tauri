<!--
  AppFormField — حاوية حقل النموذج
  الخصائص:
    label: string
    fieldId: string — لربط label بالحقل
    required: boolean
    error: string (اختياري)
    helperText: string (اختياري)
  الفتحات: default (الحقل المدار)
-->
<script lang="ts">
  export let label: string;
  export let fieldId: string;
  export let required = false;
  export let error: string | undefined = undefined;
  export let helperText: string | undefined = undefined;

  let className = '';
  export { className as class };
</script>

<div class="flex flex-col gap-1.5 {className}">
  <!-- التسمية -->
  <label
    for={fieldId}
    class="block text-sm font-medium text-gray-700 dark:text-gray-300"
  >
    {label}
    {#if required}
      <span class="text-red-500 dark:text-red-400 mr-0.5" aria-hidden="true">*</span>
    {/if}
  </label>

  <!-- الحقل المُدار -->
  <slot />

  <!-- نص مساعد أو خطأ -->
  {#if error}
    <p id="{fieldId}-error" class="text-xs text-red-600 dark:text-red-400" role="alert">{error}</p>
  {:else if helperText}
    <p id="{fieldId}-helper" class="text-xs text-gray-500 dark:text-gray-400">{helperText}</p>
  {/if}
</div>
