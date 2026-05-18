import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    conditions: ['browser', 'development']
  },
  test: {
    include: ['src/tests/unit/**/*.{test,spec}.{js,ts}', 'src/tests/integration/**/*.{test,spec}.{js,ts}'],
    globals: true,
    environment: 'jsdom',
    setupFiles: ['src/tests/helpers/setup.ts'],
  },
});
