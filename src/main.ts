import App from './App.svelte';
import './app.css';
import { mount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import { initErrorBoundary } from './lib/errorBoundary';

// Initialize error boundary before any app rendering
initErrorBoundary();

(window as any).__TAURI__ = {
  core: { invoke }
};

const app = mount(App, {
  target: document.body,
  props: {}
});

export default app;
