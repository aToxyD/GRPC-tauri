<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Sidebar from './Sidebar.svelte';

  export let nodeType: 'WILAYA' | 'UNIT' | null = null;
  export let title: string = '';
  export let subtitle: string = '';

  // Display fallback to prevent flicker
  $: displayNodeType = nodeType || 'WILAYA';

  onMount(async () => {
    try {
      const window = getCurrentWindow();
      await window.setResizable(true);
      await window.setMaximizable(true);
      if (!(await window.isMaximized())) {
        await window.maximize();
      }
      
      // Wait for window to be maximized before locking resizability/maximizability
      for (let i = 0; i < 20; i++) {
        if (await window.isMaximized()) {
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      
      await window.setResizable(false);
      await window.setMaximizable(false);
    } catch (err) {
      console.error('Failed to maximize and lock window in Layout:', err);
    }
  });
</script>

<div class="flex min-h-screen bg-gray-50">
  <Sidebar {nodeType} displayType={displayNodeType} />

  <main class="flex-1 p-8">
    {#if title}
      <div class="mb-8">
        <h1 class="text-3xl font-bold text-gray-800">{title}</h1>
        {#if subtitle}
          <p class="text-gray-600 mt-1">{subtitle}</p>
        {/if}
      </div>
    {/if}

    <slot />
  </main>
</div>
