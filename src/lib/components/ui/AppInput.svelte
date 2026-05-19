<!--
  AppInput — حقل إدخال نص موحد
  الخصائص:
    id: string
    label: string
    type: text | password | email | number | search | tel | url
    value: string
    placeholder: string
    required: boolean
    disabled: boolean
    readonly: boolean
    error: string
    helperText: string
    autocomplete: string
  الأحداث: on:input, on:change, on:blur, on:focus
-->
<script lang="ts">
  import AppFormField from './AppFormField.svelte';

  export let id: string;
  export let label: string;
  export let type: 'text' | 'password' | 'email' | 'number' | 'search' | 'tel' | 'url' | 'date' = 'text';
  export let value: string | number | null | undefined = '';
  export let placeholder = '';
  export let required = false;
  export let disabled = false;
  export let readonly = false;
  export let min: number | string | undefined = undefined;
  export let max: number | string | undefined = undefined;
  export let error: string | undefined = undefined;
  export let helperText: string | undefined = undefined;
  export let autocomplete: 'off' | 'on' | 'username' | 'current-password' | 'new-password' | 'email' | 'name' | 'tel' | 'url' | undefined = undefined;

  let className = '';
  export { className as class };

  $: inputClasses = [
    'w-full px-3 py-2 text-sm rounded-lg border transition-colors duration-150',
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

<AppFormField {label} fieldId={id} {required} {error} {helperText} class={className}>
  <input
    {id}
    {type}
    bind:value
    {placeholder}
    {required}
    {disabled}
    {min}
    {max}
    readonly={readonly}
    autocomplete={autocomplete}
    class={inputClasses}
    aria-invalid={error ? 'true' : 'false'}
    aria-describedby={error ? `${id}-error` : helperText ? `${id}-helper` : undefined}
    on:input
    on:change
    on:blur
    on:focus
    on:keydown
  />
</AppFormField>
