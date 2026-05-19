<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { configureAsWilaya, getSettings, getAppWindow, createLogicalSize } from "../lib/tauri";
  import { push } from "svelte-spa-router";
  import { formatErrorMessage } from "../lib/errors";
  import { createOperation } from "../lib/operationGuard";

  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';

  const setupOp = createOperation();
  const loading = setupOp.loading;
  const error = setupOp.error;

  let wilayaCode = "";
  let wilayaName = "";
  let success = "";
  let setupTimeout: number | null = null;

  onDestroy(() => {
    if (setupTimeout) {
      clearTimeout(setupTimeout);
    }
  });

  onMount(async () => {
    // Check if already configured
    try {
      const settings = await getSettings();
      if (settings.configured) {
        try {
          const window = getAppWindow();
          await window.setResizable(true);
          await window.setMaximizable(true);
          await window.maximize();
        } catch (err) {
          console.error("Failed to maximize window for dashboard:", err);
        }

        if (settings.node_type === "WILAYA") {
          push("/wilaya");
        } else {
          push("/unit");
        }
        return;
      }
    } catch (e) {
      // Not configured yet, continue
    }

    // Ensure the setup page runs in the compact, locked 450x650 frame
    try {
      const window = getAppWindow();
      await window.setResizable(true);
      await window.setMaximizable(true);
      if (await window.isMaximized()) {
        await window.unmaximize();
      }
      await window.setSize(createLogicalSize(450, 650));
      await window.setResizable(false);
      await window.setMaximizable(false);
      await window.center();
    } catch (err) {
      console.error("Failed to configure setup window size:", err);
    }
  });

  async function configureWilaya() {
    if (!wilayaCode || !wilayaName) {
      setupOp.error.set("الرجاء إدخال رمز واسم الولاية");
      return;
    }

    success = "";

    await setupOp.run(async () => {
      await configureAsWilaya(wilayaCode, wilayaName);
      success = "تم تكوين الولاية بنجاح. جاري إعادة التوجيه...";
      
      // Maximize the window for the main dashboard on successful configuration
      try {
        const window = getAppWindow();
        await window.setResizable(true);
        await window.setMaximizable(true);
        await window.maximize();
      } catch (err) {
        console.error("Failed to maximize window after configuration:", err);
      }

      setupTimeout = window.setTimeout(() => push("/wilaya"), 1500);
    });
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-civil-blue/5 to-civil-blue/10" dir="rtl">
  <div class="w-full max-w-lg p-4">
    <AppCard padding="lg" class="shadow-xl">
      <div class="text-center mb-8">
        <div class="w-16 h-16 bg-civil-blue rounded-full flex items-center justify-center mx-auto mb-4">
          <svg class="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"/>
          </svg>
        </div>
        <h1 class="text-2xl font-bold text-gray-800 dark:text-gray-100">تكوين الولاية</h1>
        <p class="text-gray-600 dark:text-gray-400 mt-1">قم بإعداد مديرية الولاية</p>
      </div>

      {#if $error}
        <div class="mb-6">
          <AppAlert intent="danger" dismissible on:dismiss={() => setupOp.error.set(null)}>{$error}</AppAlert>
        </div>
      {/if}

      {#if success}
        <div class="mb-6">
          <AppAlert intent="success">{success}</AppAlert>
        </div>
      {/if}

      <div class="space-y-4">
        <AppInput
          id="wilayaCode"
          label="رمز الولاية"
          placeholder="مثال: 16 (الجزائر العاصمة)"
          bind:value={wilayaCode}
          disabled={$loading}
        />

        <AppInput
          id="wilayaName"
          label="اسم الولاية"
          placeholder="مثال: الجزائر العاصمة"
          bind:value={wilayaName}
          disabled={$loading}
        />

        <div class="pt-4">
          <AppButton
            variant="primary"
            fullWidth
            size="lg"
            loading={$loading}
            disabled={$loading}
            on:click={configureWilaya}
          >
            تكوين كولاية
          </AppButton>
        </div>
      </div>
    </AppCard>
  </div>
</div>
