<!--
  AppDialog — نافذة حوار موحدة
  الخصائص:
    open: boolean
    title: string
    description: string (اختياري)
    size: sm | md | lg
    destructive: boolean — لتنسيق الإجراءات المدمرة
  الفتحات:
    default: محتوى الجسم
    actions: أزرار الإجراءات
  الأحداث: on:close
-->
<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { createRuntimeScope } from '../../runtimeCleanup';

  interface $$Slots {
    default: {};
    actions: {};
  }

  export let open = false;
  export let title: string;
  export let description: string | undefined = undefined;
  export let size: 'sm' | 'md' | 'lg' | 'xl' = 'md';
  export let destructive = false;

  const dispatch = createEventDispatcher<{ close: void }>();

  const sizeMap: Record<string, string> = {
    sm: 'max-w-sm',
    md: 'max-w-md',
    lg: 'max-w-lg',
    xl: 'max-w-3xl',
  };

  let dialogEl: HTMLDivElement;
  const scope = createRuntimeScope();

  function handleClose() {
    dispatch('close');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      handleClose();
      return;
    }

    // حصر التركيز داخل الحوار (Focus Trap)
    if (e.key === 'Tab' && dialogEl) {
      const focusable = dialogEl.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first?.focus();
      }
    }
  }

  // تركيز أول عنصر قابل للتركيز عند الفتح
  $: if (open && dialogEl) {
    const focusable = dialogEl.querySelector<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    );
    scope.setTimeout(() => focusable?.focus(), 10);
  }

  // منع التمرير خلف الحوار
  $: if (typeof document !== 'undefined') {
    document.body.style.overflow = open ? 'hidden' : '';
  }

  onDestroy(() => {
    scope.dispose();
    if (typeof document !== 'undefined') {
      document.body.style.overflow = '';
    }
  });
</script>

<svelte:window on:keydown={handleKeydown} />

{#if open}
  <!-- الخلفية -->
  <div
    class="fixed inset-0 z-40 bg-black/50 dark:bg-black/70 backdrop-blur-sm"
    aria-hidden="true"
    on:click={handleClose}
    on:keydown={() => {}}
  ></div>

  <!-- الحوار -->
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby="dialog-title"
    aria-describedby={description ? 'dialog-description' : undefined}
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
  >
    <div
      bind:this={dialogEl}
      class="w-full {sizeMap[size]} bg-white dark:bg-gray-800 rounded-xl shadow-2xl dark:shadow-gray-950/70 border border-gray-200 dark:border-gray-700 flex flex-col"
    >
      <!-- رأس الحوار -->
      <div class="flex items-start justify-between p-6 border-b border-gray-200 dark:border-gray-700">
        <div>
          <h2
            id="dialog-title"
            class="text-lg font-semibold {destructive ? 'text-red-700 dark:text-red-400' : 'text-gray-900 dark:text-gray-100'}"
          >
            {title}
          </h2>
          {#if description}
            <p id="dialog-description" class="mt-1 text-sm text-gray-500 dark:text-gray-400">
              {description}
            </p>
          {/if}
        </div>
        <button
          type="button"
          class="mr-2 p-1.5 rounded-lg text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
          aria-label="إغلاق"
          on:click={handleClose}
        >
          <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- جسم الحوار -->
      <div class="p-6 overflow-y-auto">
        <slot />
      </div>

      <!-- الإجراءات -->
      {#if $$slots.actions}
        <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/50 rounded-b-xl">
          <slot name="actions" />
        </div>
      {/if}
    </div>
  </div>
{/if}
