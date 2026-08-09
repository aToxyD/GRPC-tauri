<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { push } from 'svelte-spa-router';
  import { saveFile, getAppWindow, createLogicalSize } from '../lib/tauri';
  import {
    getSecurityStatus,
    initializeAppKey,
    unlockAppKey,
    type AppKeyStatusDto,
  } from '../lib/contracts';
  import { showSuccess } from '../lib/notifications';
  import { formatErrorMessage } from '../lib/errors';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation } from '../lib/operationGuard';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';

  const scope = createRuntimeScope();
  const op = createOperation({ scope });
  onDestroy(() => scope.dispose());
  const loading = op.loading;
  const error = op.error;

  // ADR-0041: Security Setup (Unprovisioned) / Unlock (Locked). This page is
  // reached only when the backend reports `requires_action` — the app key does
  // not resolve and DB bootstrap is deferred until the store is unlocked.
  // @category ProjectionState
  let status: AppKeyStatusDto | null = null;
  // @category UiState
  let checking = true;
  // @category TransientState
  let passphrase = '';
  // @category TransientState
  let confirmPassphrase = '';
  // @category UiState
  let exportBackup = false;
  // @category TransientState
  let localError = '';

  // @category UiState
  $: unlocked = status?.requires_action === false;
  // @category UiState
  $: setupMode = status !== null && !status.provisioned;
  // @category UiState
  $: displayError = $error || localError;

  onMount(async () => {
    try {
      const window = getAppWindow();
      await window.setResizable(true);
      await window.setMaximizable(true);
      if (await window.isMaximized()) await window.unmaximize();
      await window.setSize(createLogicalSize(520, 820));
      await window.setResizable(false);
      await window.setMaximizable(false);
      await window.center();
    } catch (err) {
      console.error('Failed to configure security window size:', err);
    }

    try {
      status = await getSecurityStatus();
    } catch (e) {
      localError = 'تعذر قراءة حالة الأمان: ' + formatErrorMessage(e);
    }
    checking = false;

    // Backend projection: no action needed → normal login flow.
    if (status && !status.requires_action) {
      push('/login');
    }
  });

  async function handleUnlock() {
    if (!passphrase) {
      localError = 'أدخل كلمة مرور المفتاح';
      return;
    }
    localError = '';
    await op.run(async () => {
      await unlockAppKey(passphrase);
      passphrase = '';
      showSuccess('تم فتح التطبيق بنجاح. يمكنك الآن تسجيل الدخول.');
      push('/login');
    });
  }

  async function handleInitialize() {
    if (passphrase.length < 8) {
      localError = 'كلمة المرور قصيرة جداً — يجب أن تكون 8 أحرف على الأقل';
      return;
    }
    if (passphrase !== confirmPassphrase) {
      localError = 'كلمتا المرور غير متطابقتين';
      return;
    }
    localError = '';
    await op.run(async () => {
      let backupPath: string | null = null;
      if (exportBackup) {
        const selected = await saveFile({
          defaultPath: 'grpc-app-key.age',
          filters: [{ name: 'نسخة احتياطية للمفتاح (age)', extensions: ['age'] }],
        });
        if (!selected) return;
        backupPath = selected as string;
      }
      await initializeAppKey(passphrase, backupPath);
      passphrase = '';
      confirmPassphrase = '';
      showSuccess('تم إعداد مفتاح التطبيق بنجاح. احتفظ بنسخة المفتاح في مكان آمن.');
      push('/login');
    });
  }
</script>

<div
  class="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900"
  dir="rtl"
>
  <div class="w-full max-w-md">
    <AppCard elevated padding="lg">
      <div class="text-center mb-8">
        <div
          class="w-16 h-16 bg-civil-blue rounded-full flex items-center justify-center mx-auto mb-4 shadow-lg"
          role="img"
          aria-label="قفل أمان التطبيق"
        >
          <svg class="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/>
          </svg>
        </div>
        <h1 class="text-2xl font-bold text-gray-900 dark:text-white">أمان التطبيق</h1>
        <p class="text-gray-500 dark:text-gray-400 mt-1 text-sm">
          {setupMode ? 'إعداد مفتاح التشفير لأول مرة' : 'فتح مفتاح التشفير'}
        </p>
      </div>

      {#if checking}
        <p class="text-center text-gray-500 dark:text-gray-400 text-sm">جارٍ التحقق من الحالة…</p>
      {:else if unlocked}
        <AppAlert intent="success">
          <p class="text-sm font-semibold">مفتاح التطبيق مفتوح — لا إجراء مطلوب.</p>
        </AppAlert>
        <div class="mt-6">
          <AppButton variant="primary" size="lg" fullWidth on:click={() => push('/login')}>
            الانتقال إلى تسجيل الدخول
          </AppButton>
        </div>
      {:else}
        {#if displayError}
          <div class="mb-4">
            <AppAlert intent="danger">{displayError}</AppAlert>
          </div>
        {/if}

        {#if status?.store_path}
          <div class="mb-4">
            <AppAlert intent="info">
              <div class="space-y-1">
                <p class="text-sm font-semibold">
                  {setupMode ? 'الملف سيُنشأ في:' : 'الملف الحالي:'}
                </p>
                <p class="text-xs break-all">{status.store_path}</p>
              </div>
            </AppAlert>
          </div>
        {/if}

        {#if setupMode}
          <form class="space-y-4" on:submit|preventDefault={handleInitialize} novalidate>
            <AppAlert intent="warning">
              <p class="text-sm leading-relaxed">
                سيتم توليد مفتاح تشفير جديد وتخزينه محمياً بكلمة مرور. <b>فقدان كلمة المرور
                يعني فقدان البيانات</b> — احتفظ بها في مكان آمن.
              </p>
            </AppAlert>

            <AppInput
              id="security-passphrase"
              label="كلمة مرور المفتاح"
              type="password"
              bind:value={passphrase}
              placeholder="8 أحرف على الأقل"
              autocomplete="new-password"
              required
              disabled={$loading}
            />

            <AppInput
              id="security-confirm-passphrase"
              label="تأكيد كلمة المرور"
              type="password"
              bind:value={confirmPassphrase}
              placeholder="أعد إدخال كلمة المرور"
              autocomplete="new-password"
              required
              disabled={$loading}
            />

            <label class="flex items-center gap-3 cursor-pointer select-none">
              <input
                type="checkbox"
                bind:checked={exportBackup}
                class="w-4 h-4 text-civil-blue focus:ring-civil-blue"
              />
              <span class="text-sm text-gray-700 dark:text-gray-300">
                تصدير نسخة احتياطية من المفتاح (مرة واحدة فقط)
              </span>
            </label>

            <AppButton type="submit" variant="primary" size="lg" fullWidth loading={$loading}>
              إنشاء المفتاح وفتح التطبيق
            </AppButton>
          </form>
        {:else}
          <form class="space-y-4" on:submit|preventDefault={handleUnlock} novalidate>
            <AppAlert intent="warning">
              <p class="text-sm leading-relaxed">
                التطبيق مغلق. أدخل كلمة مرور مفتاح التشفير لفتح قاعدة البيانات.
                إدخال كلمة مرور خاطئة يُبقي التطبيق مقفلاً.
              </p>
            </AppAlert>

            <AppInput
              id="security-unlock-passphrase"
              label="كلمة مرور المفتاح"
              type="password"
              bind:value={passphrase}
              placeholder="أدخل كلمة مرور المفتاح"
              autocomplete="current-password"
              required
              disabled={$loading}
            />

            <AppButton type="submit" variant="primary" size="lg" fullWidth loading={$loading}>
              فتح التطبيق
            </AppButton>
          </form>
        {/if}
      {/if}
    </AppCard>
  </div>
</div>
