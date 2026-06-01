<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  // [arch:allow-component-ipc] window management only (getAppWindow, listenToResize) — no business data
  import { getAppWindow, listenToResize } from '../lib/tauri';
  import Sidebar from './Sidebar.svelte';

  export let nodeType: 'WILAYA' | 'UNIT' | null = null;
  export let title: string = '';
  export let subtitle: string = '';

  // Display fallback to prevent flicker
  // @category UiState
  $: displayNodeType = nodeType || 'WILAYA';

  // @category UiState — cleanup handle for resize listener
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    try {
      const window = getAppWindow();
      await window.setResizable(true);
      await window.setMaximizable(true);
      if (!(await window.isMaximized())) {
        await window.maximize();
      }
      
      // Wait for window to be maximized before locking resizability/maximizability using event listener
      unlisten = await listenToResize(async () => {
        if (await window.isMaximized()) {
          await window.setResizable(false);
          await window.setMaximizable(false);
          if (unlisten) {
            unlisten();
            unlisten = null;
          }
        }
      });
    } catch (err) {
      console.error('Failed to maximize and lock window in Layout:', err);
    }
  });

  onDestroy(() => {
    if (unlisten) {
      unlisten();
    }
  });
</script>

<div class="flex min-h-screen bg-gray-50 dark:bg-gray-900 transition-colors duration-200">
  <Sidebar {nodeType} displayType={displayNodeType} />

  <main class="flex-1 p-8">
    {#if title}
      <div class="mb-8">
        <h1 class="text-3xl font-bold text-gray-800 dark:text-gray-100">{title}</h1>
        {#if subtitle}
          <p class="text-gray-600 dark:text-gray-400 mt-1">{subtitle}</p>
        {/if}
      </div>
    {/if}

    <slot />
  </main>
</div>
