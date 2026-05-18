import App from './App.svelte';
import './app.css';
import { mount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';

(window as any).__TAURI__ = {
  core: { invoke }
};

const app = mount(App, {
  target: document.body,
  props: {}
});

export default app;
