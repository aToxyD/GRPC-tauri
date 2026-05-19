<!--
  AppCard — حاوية محتوى موحدة
  الخصائص:
    elevated: boolean — ظل مرتفع
    padding: none | sm | md | lg
    noBorder: boolean — بدون حد
-->
<script lang="ts">
  export let elevated = false;
  export let padding: 'none' | 'sm' | 'md' | 'lg' = 'md';
  export let noBorder = false;

  let className = '';
  export { className as class };

  const paddingMap: Record<string, string> = {
    none: '',
    sm: 'p-4',
    md: 'p-6',
    lg: 'p-8',
  };

  $: classes = [
    'bg-white dark:bg-gray-800 rounded-xl transition-colors duration-200',
    elevated
      ? 'shadow-lg dark:shadow-gray-950/50'
      : 'shadow-sm dark:shadow-gray-950/30',
    noBorder ? '' : 'border border-gray-200 dark:border-gray-700',
    paddingMap[padding],
    className,
  ]
    .filter(Boolean)
    .join(' ');
</script>

<div class={classes}>
  <slot />
</div>
