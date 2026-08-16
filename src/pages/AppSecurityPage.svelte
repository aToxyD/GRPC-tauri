<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { push } from 'svelte-spa-router';
  import { saveFile, openFile, getAppWindow, createLogicalSize } from '../lib/tauri';
  import {
    getSecurityStatus,
    initializeAppKey,
    importAppKey,
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
  // @category TransientState
  let importPassphrase = '';
  // @category TransientState
  let importArtifactPath = '';
  // @category UiState
  let importSuccess = false;
  // @category TransientState
  let importError = '';

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

  // APPKEY-003: fleet-path import — pick the WILAYA portable artifact; the
  // backend reads, validates, and encrypts it into the local store. The raw
  // identity never enters the DOM or IPC (only the path is sent).
  async function pickArtifact() {
    const selected = await openFile({
      multiple: false,
      filters: [{ name: 'مفتاح الأسطول المحمول (age)', extensions: ['age', 'key', 'txt'] }],
    });
    if (selected) {
      importArtifactPath = selected as string;
      importError = '';
    }
  }

  async function handleImport() {
    if (!importArtifactPath) {
      importError = 'اختر ملف grpc-app-key.age أولاً';
      return;
    }
    if (importPassphrase.length < 8) {
      importError = 'كلمة مرور التخزين المحلي يجب أن تكون 8 أحرف على الأقل';
      return;
    }
    importError = '';
    importSuccess = false;
    try {
      await op.run(async () => {
        await importAppKey(importPassphrase, importArtifactPath);
        importPassphrase = '';
        importArtifactPath = '';
        importSuccess = true;
        showSuccess('تم استيراد مفتاح الأسطول وفتح التطبيق. يمكنك الآن تسجيل الدخول.');
        push('/login');
      });
    } catch (e) {
      importError = formatErrorMessage(e);
    }
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
          {#if status?.source === 'env'}
            <p class="text-xs mt-1">مفتاح الأسطول متوفر عبر GRPC_APP_KEY (لا يُعرض السر هنا).</p>
          {:else if status?.source === 'store'}
            <p class="text-xs mt-1">المصدر: المخزن المحلي appkey.age.</p>
          {:else if status?.source === 'dev'}
            <p class="text-xs mt-1">المصدر: مفتاح تطوير مضمّن — ليس للإنتاج.</p>
          {/if}
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

        {#if status?.source === 'env'}
          <div class="mb-4">
            <AppAlert intent="success">
              <p class="text-sm font-semibold">مفتاح الأسطول متوفر عبر GRPC_APP_KEY</p>
              <p class="text-xs mt-1">
                المفتاح مُوفَّر من بيئة التشغيل (نفس قيمة WILAYA) ولا يُعرض هنا — لا حاجة
                لإعداد محلي.
              </p>
            </AppAlert>
          </div>
        {:else if status?.source === 'store'}
          <div class="mb-4">
            <AppAlert intent="info">
              <p class="text-sm font-semibold">مخزن المفتاح المحلي متوفر</p>
              <p class="text-xs mt-1">أدخل كلمة المرور لفتح المخزن المحلي (appkey.age).</p>
            </AppAlert>
          </div>
        {:else if status?.source === 'dev'}
          <div class="mb-4">
            <AppAlert intent="warning">
              <p class="text-sm font-semibold">مفتاح تطوير مضمّن — ليس للإنتاج</p>
              <p class="text-xs mt-1">وضع التطوير فقط؛ وفّر GRPC_APP_KEY أو أنشئ مخزناً محلياً للإنتاج.</p>
            </AppAlert>
          </div>
        {/if}

        {#if setupMode && status?.source === 'none'}
          <div class="mb-4 space-y-3">
            <AppAlert intent="danger">
              <p class="text-sm font-semibold">لا يوجد مفتاح App متاح بعد</p>
              <p class="text-xs mt-1 leading-relaxed">
                إذا كانت هذه العقدة وحدة (UNIT) تابعة لـ WILAYA، يجب توفير
                <code class="font-mono">GRPC_APP_KEY</code> بنفس قيمة مفتاح WILAYA
                <b>قبل أول تشغيل</b>. إنشاء مفتاح محلي جديد يختلف عن مفتاح WILAYA
                وسيؤدي إلى <b>فشل فك تشفير حزمة .unit</b> عند الاستيراد (Fail-Closed).
              </p>
            </AppAlert>
            <AppCard>
              <p class="text-sm font-semibold text-gray-900 dark:text-white">
                مسار الأسطول — لديّ مفتاح WILAYA (موصى به لعقد UNIT)
              </p>
              <ol class="text-xs text-gray-600 dark:text-gray-300 mt-2 list-decimal list-inside space-y-1">
                <li>
                  احصل على قيمة مفتاح الأسطول من WILAYA (أداة التوفير المحمولة
                  <code class="font-mono"> grpc-app-key.age</code>).
                </li>
                <li>
                  اضبط متغير البيئة <code class="font-mono">GRPC_APP_KEY</code> بنفس
                  القيمة تماماً.
                </li>
                <li>
                  أعد تشغيل التطبيق — سيعرض هذا القسم حالة «مفتاح الأسطول متوفر عبر
                  GRPC_APP_KEY».
                </li>
                <li>
                  لا تنشئ مفتاحاً محلياً في مسار الأسطول: المفتاح المحلي الجديد مختلف
                  عن مفتاح WILAYA ويفشل فك تشفير <code class="font-mono">.unit</code>.
                </li>
              </ol>
              <div class="mt-3 border-t border-gray-200 dark:border-gray-700 pt-3">
                <p class="text-xs text-gray-600 dark:text-gray-300 mb-2">
                  أو استورد الملف المحمول مباشرة: يقرأ التطبيق
                  <code class="font-mono">grpc-app-key.age</code> من WILAYA ويخزنه
                  محلياً مشفراً بكلمة مرور — بدون تعديل الملف الأصلي.
                </p>
                {#if importSuccess}
                  <AppAlert intent="success">
                    <p class="text-sm">تم استيراد مفتاح الأسطول بنجاح.</p>
                  </AppAlert>
                {/if}
                {#if importError}
                  <AppAlert intent="danger">
                    <p class="text-sm">{importError}</p>
                  </AppAlert>
                {/if}
                <form class="space-y-2" on:submit|preventDefault={handleImport}>
                  <div class="flex items-center gap-2">
                    <AppButton
                      type="button"
                      variant="secondary"
                      size="sm"
                      disabled={$loading}
                      on:click={pickArtifact}
                    >
                      اختيار الملف…
                    </AppButton>
                    {#if importArtifactPath}
                      <code class="text-xs text-gray-600 dark:text-gray-300 truncate flex-1" dir="ltr">
                        {importArtifactPath}
                      </code>
                    {:else}
                      <span class="text-xs text-gray-400 flex-1">لم يتم اختيار ملف بعد</span>
                    {/if}
                  </div>
                  <AppInput
                    id="security-import-passphrase"
                    label="كلمة مرور التخزين المحلي"
                    type="password"
                    bind:value={importPassphrase}
                    placeholder="8 أحرف على الأقل"
                    autocomplete="new-password"
                    disabled={$loading}
                  />
                  <AppButton type="submit" variant="primary" size="sm" fullWidth loading={$loading}>
                    استيراد المفتاح وفتح التطبيق
                  </AppButton>
                </form>
              </div>
            </AppCard>
            <div class="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
              <span class="flex-1 border-t border-gray-300 dark:border-gray-600"></span>
              أو — إعداد مفتاح محلي (عقدة مستقلة / WILAYA)
              <span class="flex-1 border-t border-gray-300 dark:border-gray-600"></span>
            </div>
          </div>
        {/if}

        {#if setupMode}
          <form class="space-y-4" on:submit|preventDefault={handleInitialize} novalidate>
            <AppAlert intent="warning">
              <p class="text-sm leading-relaxed">
                سيتم توليد مفتاح تشفير جديد وتخزينه محمياً بكلمة مرور. <b>فقدان كلمة المرور
                يعني فقدان البيانات</b> — احتفظ بها في مكان آمن.
              </p>
              {#if status?.source === 'none'}
                <p class="text-xs mt-2 leading-relaxed">
                  هذا المسار صحيح فقط للعقد المستقلة أو عقدة WILAYA. إذا كانت العقدة UNIT
                  تابعة للأسطول، استخدم مسار <code class="font-mono">GRPC_APP_KEY</code>
                  أعلاه — المفتاح المحلي المتباين سيفشل فك تشفير حزمة
                  <code class="font-mono">.unit</code>.
                </p>
              {/if}
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
