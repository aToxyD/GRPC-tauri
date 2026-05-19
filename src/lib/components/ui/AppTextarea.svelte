<!--
  AppTextarea — حقل نص متعدد الأسطر
  الخصائص:
    id: string
    label: string
    value: string
    placeholder: string
    rows: number
    required: boolean
    disabled: boolean
    readonly: boolean
    error: string
    helperText: string
  الأحداث: on:input, on:change, on:blur
-->
<script lang="ts">
  import AppFormField from './AppFormField.svelte';

  export let id: string;
  export let label: string;
  export let value = '';
  export let placeholder = '';
  export let rows = 4;
  export let required = false;
  export let disabled = false;
  export let readonly = false;
  export let error: string | undefined = undefined;
  export let helperText: string | undefined = undefined;

  $: textareaClasses = [
    'w-full px-3 py-2 text-sm rounded-lg border transition-colors duration-150 resize-y',
    'bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100',
    'placeholder:text-gray-400 dark:placeholder:text-gray-500',
    'focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 focus:border-transparent',
    error
      ? 'border-red-400 dark:border-red-600'
      : 'border-gray-300 dark:border-gray-600',
    disabled
      ? 'bg-gray-50 dark:bg-gray-800 text-gray-400 dark:text-gray-500 cursor-not-allowed'
      : '',
  ]
    .filter(Boolean)
    .join(' ');
</script>

<AppFormField {label} fieldId={id} {required} {error} {helperText}>
  <textarea
    {id}
    bind:value
    {placeholder}
    {rows}
    {required}
    {disabled}
    readonly={readonly}
    class={textareaClasses}
    aria-invalid={error ? 'true' : 'false'}
    aria-describedby={error ? `${id}-error` : helperText ? `${id}-helper` : undefined}
    on:input
    on:change
    on:blur
  ></textarea>
</AppFormField>
