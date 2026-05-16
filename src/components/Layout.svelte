<script lang="ts">
  import Sidebar from './Sidebar.svelte';

  export let nodeType: 'WILAYA' | 'UNIT' | null = null;
  export let title: string = '';
  export let subtitle: string = '';

  // Fallback to localStorage to prevent flicker/disappearance during loading
  // Display fallback to prevent flicker
  $: displayNodeType = nodeType || (typeof localStorage !== 'undefined' ? localStorage.getItem('grpc_node_type') as 'WILAYA' | 'UNIT' : null) || 'WILAYA';
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
