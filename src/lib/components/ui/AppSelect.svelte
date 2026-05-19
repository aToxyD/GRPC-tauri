<!--
  AppSelect — قائمة اختيار موحدة
  الخصائص:
    id: string
    label: string
    value: string
    required: boolean
    disabled: boolean
    error: string
    helperText: string
  الفتحات: default (عناصر option)
  الأحداث: on:change
-->
<script lang="ts">
  import AppFormField from './AppFormField.svelte';

  export let id: string;
  export let label: string;
  export let value: string | number | undefined = '';
  export let required = false;
  export let disabled = false;
  export let error: string | undefined = undefined;
  export let helperText: string | undefined = undefined;

  $: selectClasses = [
    'w-full px-3 py-2 text-sm rounded-lg border transition-colors duration-150 appearance-none',
    'bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100',
    'focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 focus:border-transparent',
    error
      ? 'border-red-400 dark:border-red-600'
      : 'border-gray-300 dark:border-gray-600',
    disabled
      ? 'bg-gray-50 dark:bg-gray-800 text-gray-400 dark:text-gray-500 cursor-not-allowed'
      : 'cursor-pointer',
  ]
    .filter(Boolean)
    .join(' ');
</script>

<AppFormField {label} fieldId={id} {required} {error} {helperText}>
  <div class="relative">
    <select
      {id}
      bind:value
      {required}
      {disabled}
      class={selectClasses}
      aria-invalid={error ? 'true' : 'false'}
      aria-describedby={error ? `${id}-error` : helperText ? `${id}-helper` : undefined}
      on:change
    >
      <slot />
    </select>
    <!-- سهم الاختيار -->
    <div class="pointer-events-none absolute inset-y-0 left-3 flex items-center" aria-hidden="true">
      <svg class="h-4 w-4 text-gray-400 dark:text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </div>
  </div>
</AppFormField>
