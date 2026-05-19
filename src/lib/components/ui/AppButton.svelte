<!--
  AppButton — زر إجراء موحد
  الخصائص:
    variant: primary | secondary | danger | ghost
    size: sm | md | lg
    type: button | submit | reset
    disabled: boolean
    loading: boolean
    fullWidth: boolean
    ariaLabel: string
  الأحداث: on:click, on:keydown
-->
<script lang="ts">
  export let variant: 'primary' | 'secondary' | 'danger' | 'ghost' = 'primary';
  export let size: 'sm' | 'md' | 'lg' = 'md';
  export let type: 'button' | 'submit' | 'reset' = 'button';
  export let disabled = false;
  export let loading = false;
  export let fullWidth = false;
  export let ariaLabel: string | undefined = undefined;
  export let title: string | undefined = undefined;

  let className = '';
  export { className as class };

  const baseClasses =
    'inline-flex items-center justify-center gap-2 font-medium rounded-lg transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-blue-500 dark:focus-visible:ring-blue-400 disabled:opacity-50 disabled:cursor-not-allowed select-none';

  const variantMap: Record<string, string> = {
    primary:
      'bg-civil-blue text-white hover:bg-civil-blue-dark active:bg-civil-blue-dark',
    secondary:
      'bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-100 hover:bg-gray-200 dark:hover:bg-gray-600 border border-gray-300 dark:border-gray-600',
    danger:
      'bg-red-600 text-white hover:bg-red-700 active:bg-red-800 focus-visible:ring-red-500',
    ghost:
      'bg-transparent text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-white',
  };

  const sizeMap: Record<string, string> = {
    sm: 'px-3 py-1.5 text-xs',
    md: 'px-4 py-2 text-sm',
    lg: 'px-5 py-2.5 text-base',
  };

  $: classes = [
    baseClasses,
    variantMap[variant],
    sizeMap[size],
    fullWidth ? 'w-full' : '',
    className,
  ].filter(Boolean).join(' ');
</script>

<button
  {type}
  class={classes}
  disabled={disabled || loading}
  aria-disabled={disabled || loading}
  aria-label={ariaLabel}
  aria-busy={loading}
  title={title}
  on:click
  on:keydown
>
  {#if loading}
    <svg
      class="animate-spin h-4 w-4 flex-shrink-0"
      fill="none"
      viewBox="0 0 24 24"
      aria-hidden="true"
    >
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
      <path
        class="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>
  {/if}
  <slot />
</button>
