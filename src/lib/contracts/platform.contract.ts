// Platform contract: infrastructure-level wrappers (window, dialogs) remain in tauri.ts
// No IPC wrappers to migrate — platform helpers are direct Tauri plugin calls, not safeInvoke.
export {};
