// Global type declarations for the project

// Declare CSS modules
declare module '*.css' {
  const content: string;
  export default content;
}

// Declare Svelte modules
declare module '*.svelte' {
  import { SvelteComponent } from 'svelte';
  export default SvelteComponent;
}
