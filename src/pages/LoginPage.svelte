<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { openFile, saveFile, getAppWindow, createLogicalSize } from '../lib/tauri';
  import {
    importUnitNodePackage,
    login,
    getSettings,
    isConfigured,
    getIdentityStatus,
    getSecurityStatus,
    beginWilayaProvision,
    finalizeWilayaProvision,
    issueFirstAdminKey,
    beginUnitProvision,
    finalizeUnitProvision,
    installWilayaCertificate,
    beginChallenge,
    completeChallenge,
  } from '../lib/contracts';
  import type { IdentityBootstrapState } from '../lib/contracts';
  import type { LoginRequest, LoginResponse } from '../lib/types';
  import { push } from 'svelte-spa-router';
  import { showSuccess } from '../lib/notifications';
  import { setCurrentUser } from '../lib/session';
  import { formatErrorMessage } from '../lib/errors';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const loginOp = createOperation({ scope });
  const loginLoading = loginOp.loading;
  const loginError = loginOp.error;

  const importOp = createOperationGuard({ scope });
  const importLoading = importOp.loading;

  // @category TransientState
  let username = '';
  // @category TransientState
  let password = '';
  // @category TransientState
  let localError = '';
  // @category ProjectionState
  let isAppConfigured = true;
  // @category ProjectionState
  let isUnitNode = false;

  // @category ProjectionState
  let loginAttempts = 0;
  // @category ProjectionState
  let remainingAttempts: number | null = null;
  // @category ProjectionState
  let lockoutTimeRemaining: number | null = null;
  // @category ProjectionState
  let isRateLimited = false;

  // ADR-0041 §11.4 (SEC-013 Phase 2): when the app key resolves from the OS
  // keyring, show a notice that auto-unlock is enabled + a link to the
  // security surface (manage / forget). The key itself is never displayed.
  // @category ProjectionState
  let appKeySource: string | null = null;

  // B5 identity bootstrap (RFC 2026-08-04 §3.6–3.7 / ADR-0038)
  // @category ProjectionState
  let identityState: IdentityBootstrapState = 'UNINITIALIZED';
  // @category UiState
  let authTab: 'password' | 'adminkey' = 'password';
  // @category UiState
  let authTabDirty = false;
  // @category TransientState
  let passphrase = '';
  // @category TransientState
  let bootstrapUsername = 'admin';
  // @category TransientState
  let bootstrapPassphrase = '';

  const bootstrapOp = createOperationGuard({ scope });
  const bootstrapLoading = bootstrapOp.loading;

  // ADR-0050 (SEC-013): password is the normal login path on every node —
  // WILAYA Admin authenticates with the B8 fleet credential. Challenge–
  // Response (`.adminkey`) remains available as the recovery / high-assurance
  // path (admin-key tab). The backend gate still routes accounts without a
  // usable password credential (identity-only ADMIN ceremony) to the challenge.
  // @category UiState
  $: adminkeyAvailable =
    identityState === 'WILAYA_ACTIVE' ||
    identityState === 'ADMIN_PROVISIONED' ||
    identityState === 'READY';
  // @category UiState
  $: wilayaBootstrapActive =
    !isUnitNode &&
    (identityState === 'UNINITIALIZED' ||
      identityState === 'WAITING_FOR_ROOT_CERTIFICATE' ||
      identityState === 'WILAYA_ACTIVE');
  // @category UiState
  $: unitBootstrapActive =
    isUnitNode &&
    (identityState === 'UNINITIALIZED' ||
      identityState === 'UNIT_WAITING_FOR_CERTIFICATE' ||
      identityState === 'UNIT_ACTIVE');
  // B8 anchor-first (ADR-0045): on a fresh (unconfigured) node the identity
  // projection walks the default WILAYA chain, so an ACTIVE WILAYA certificate
  // — the local trust anchor — is present exactly when `identityState` reports
  // WILAYA_ACTIVE. Presentation-only gate; the backend B8 predicate remains the
  // authoritative enforcement.
  // @category UiState
  $: unitAnchorInstalled = identityState === 'WILAYA_ACTIVE';

  // @category UiState
  $: displayError = $loginError || localError;

  function bootstrapStatusLabel(state: IdentityBootstrapState): string {
    switch (state) {
      case 'UNINITIALIZED':
        return 'لم تبدأ تهيئة الهوية بعد';
      case 'WAITING_FOR_ROOT_CERTIFICATE':
        return 'بانتظار توقيع المرجع — استورد الشهادة الموقعة';
      case 'WILAYA_ACTIVE':
        return 'شهادة العقدة (WILAYA) مفعلة';
      case 'ADMIN_PROVISIONED':
        return 'المفتاح الإداري صدر — يمكنك تسجيل الدخول';
      case 'READY':
        return 'الهوية جاهزة';
      case 'UNIT_WAITING_FOR_CERTIFICATE':
        return 'بانتظار شهادة WILAYA — استورد الشهادة الموقعة';
      case 'UNIT_ACTIVE':
        return 'هوية الوحدة مفعلة';
    }
  }

  async function refreshNodeType() {
    try {
      const settings = await getSettings();
      isUnitNode = settings.configured && settings.node_type === 'UNIT';
    } catch {
      isUnitNode = false;
    }
  }

  async function refreshIdentityStatus() {
    try {
      identityState = await getIdentityStatus();
    } catch (e) {
      localError = 'تعذر قراءة حالة الهوية: ' + formatErrorMessage(e);
    }
  }

  onMount(async () => {
    // ADR-0041: if the app key does not resolve (locked store / unprovisioned),
    // route to the Security Setup / Unlock page — the DB is deferred and no
    // command below can run yet.
    try {
      const security = await getSecurityStatus();
      appKeySource = security.source;
      if (security.requires_action) {
        push('/security');
        return;
      }
    } catch (e) {
      console.error('Failed to read security status:', e);
    }
    try {
      isAppConfigured = await isConfigured();
    } catch (e) {
      isAppConfigured = false;
    }
    await refreshNodeType();
    await refreshIdentityStatus();
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
      console.error('Failed to configure login window size:', err);
    }
  });

  async function handleImportPackage() {
    await importOp.guard(async () => {
      try {
        localError = '';
        loginOp.error.set(null);
        const selected = await openFile({
          multiple: false,
          filters: [{ name: 'حزمة التكوين', extensions: ['unit'] }],
        });
        if (selected) {
          await importUnitNodePackage(selected as string);
          isAppConfigured = true;
          await refreshNodeType();
          await refreshIdentityStatus();
          showSuccess('تم استيراد حزمة التكوين بنجاح! يمكنك الآن تسجيل الدخول.');
        }
      } catch (e) {
        localError = 'خطأ في استيراد الحزمة: ' + formatErrorMessage(e);
      }
    });
  }

  async function handleBeginWilaya() {
    await bootstrapOp.guard(async () => {
      try {
        localError = '';
        const filePath = await saveFile({
          defaultPath: 'grpc-wilaya-request.json',
          filters: [{ name: 'طلب توقيع العقدة (JSON)', extensions: ['json'] }],
        });
        if (!filePath) return;
        await beginWilayaProvision(filePath as string);
        await refreshIdentityStatus();
        showSuccess('تم إنشاء طلب التوقيع وحفظه. سلّمه إلى المرجع لاعتماد العقدة.');
      } catch (e) {
        localError = 'خطأ في بدء تهيئة الهوية: ' + formatErrorMessage(e);
      }
    });
  }

  async function handleFinalizeWilaya() {
    await bootstrapOp.guard(async () => {
      try {
        localError = '';
        const selected = await openFile({
          multiple: false,
          filters: [{ name: 'شهادة موقعة (JSON)', extensions: ['json'] }],
        });
        if (!selected) return;
        const result = await finalizeWilayaProvision(selected as string);
        await refreshIdentityStatus();
        const provisioned = 'Provisioned' in result;
        showSuccess(
          provisioned
            ? 'تم تفعيل شهادة العقدة (WILAYA).'
            : 'الشهادة مطابقة لما تم استيراده مسبقاً.'
        );
      } catch (e) {
        localError = 'خطأ في تفعيل الشهادة: ' + formatErrorMessage(e);
      }
    });
  }

  async function handleIssueAdminKey() {
    if (!bootstrapUsername.trim() || !bootstrapPassphrase) {
      localError = 'أدخل اسم المستخدم وكلمة مرور المفتاح';
      return;
    }
    await bootstrapOp.guard(async () => {
      try {
        localError = '';
        await issueFirstAdminKey(bootstrapUsername.trim(), bootstrapPassphrase);
        bootstrapPassphrase = '';
        await refreshIdentityStatus();
        showSuccess('تم إصدار المفتاح الإداري. يمكنك الآن تسجيل الدخول عبر المفتاح.');
      } catch (e) {
        localError = 'خطأ في إصدار المفتاح الإداري: ' + formatErrorMessage(e);
      }
    });
  }

  // UNIT bootstrap (RFC §3.12, B6-A) — strict two-step: the WILAYA certificate
  // is NEVER bundled; the operator installs it as the local trust anchor and
  // then finalizes the UNIT certificate, both in dedicated steps.
  async function handleBeginUnit() {
    await bootstrapOp.guard(async () => {
      try {
        localError = '';
        const filePath = await saveFile({
          defaultPath: 'grpc-unit-request.json',
          filters: [{ name: 'طلب توقيع الوحدة (JSON)', extensions: ['json'] }],
        });
        if (!filePath) return;
        await beginUnitProvision(filePath as string);
        await refreshIdentityStatus();
        showSuccess('تم إنشاء طلب توقيع الوحدة وحفظه. سلّمه إلى عقدة WILAYA للاعتماد.');
      } catch (e) {
        localError = 'خطأ في بدء تهيئة الوحدة: ' + formatErrorMessage(e);
      }
    });
  }

  async function handleInstallWilayaCert() {
    await bootstrapOp.guard(async () => {
      try {
        localError = '';
        const selected = await openFile({
          multiple: false,
          filters: [{ name: 'شهادة WILAYA (JSON)', extensions: ['json'] }],
        });
        if (!selected) return;
        const result = await installWilayaCertificate(selected as string);
        await refreshIdentityStatus();
        const installed = 'Installed' in result;
        showSuccess(
          installed
            ? 'تم تثبيت شهادة WILAYA كمرساة ثقة محلية.'
            : 'الشهادة مطابقة لما تم تثبيته مسبقاً.'
        );
      } catch (e) {
        localError = 'خطأ في تثبيت شهادة WILAYA: ' + formatErrorMessage(e);
      }
    });
  }

  async function handleFinalizeUnit() {
    await bootstrapOp.guard(async () => {
      try {
        localError = '';
        const selected = await openFile({
          multiple: false,
          filters: [{ name: 'شهادة موقعة (JSON)', extensions: ['json'] }],
        });
        if (!selected) return;
        const result = await finalizeUnitProvision(selected as string);
        await refreshIdentityStatus();
        const provisioned = 'Provisioned' in result;
        showSuccess(
          provisioned
            ? 'تم تفعيل هوية الوحدة.'
            : 'الشهادة مطابقة لما تم استيراده مسبقاً.'
        );
      } catch (e) {
        localError = 'خطأ في تفعيل هوية الوحدة: ' + formatErrorMessage(e);
      }
    });
  }

  async function afterLogin(response: LoginResponse) {
    loginAttempts = 0;
    remainingAttempts = null;
    isRateLimited = false;
    setCurrentUser(response.user);
    if (response.requires_configuration) {
      push('/configure?nodeType=WILAYA');
      return;
    }
    try {
      const window = getAppWindow();
      await window.setResizable(true);
      await window.setMaximizable(true);
      await window.maximize();
    } catch (err) {
      console.error('Failed to maximize window:', err);
    }
    try {
      const settings = await getSettings();
      if (settings?.node_type === 'WILAYA') push('/wilaya');
      else if (settings?.node_type === 'UNIT') push('/unit');
      else push('/configure');
    } catch {
      push('/configure');
    }
  }

  async function handleChallengeLogin() {
    if (!passphrase) {
      localError = 'أدخل كلمة مرور المفتاح الإداري';
      return;
    }
    if (isRateLimited) {
      localError = `تم حظر تسجيل الدخول مؤقتاً. انتظر ${lockoutTimeRemaining || 5} دقائق`;
      return;
    }
    localError = '';
    await loginOp.run(async () => {
      const challenge = await beginChallenge();
      const response = await completeChallenge(challenge.session_id, passphrase);
      if (response.success && response.user) {
        await afterLogin(response);
      } else {
        throw new Error(response.message || 'فشل تسجيل الدخول عبر المفتاح الإداري');
      }
    });
  }

  async function handleLogin() {
    if (!username || !password) {
      localError = 'الرجاء إدخال اسم المستخدم وكلمة المرور';
      return;
    }
    if (isRateLimited) {
      localError = `تم حظر تسجيل الدخول مؤقتاً. انتظر ${lockoutTimeRemaining || 5} دقائق`;
      return;
    }
    localError = '';
    
    await loginOp.run(async () => {
      const request: LoginRequest = { username, password };
      const response: LoginResponse = await login(request);
      if (response.identity_challenge_required) {
        // The backend gate routed an account WITHOUT a usable password
        // credential (identity-only ADMIN ceremony) to Challenge–Response.
        // Route the operator to the admin-key challenge.
        authTab = 'adminkey';
        authTabDirty = true;
        localError = response.message || 'هذه العقدة تتطلب تسجيل الدخول بالمفتاح الإداري';
        return;
      }
      if (response.success && response.user) {
        await afterLogin(response);
      } else {
        const msg = response.message || 'بيانات الدخول غير صالحة';
        localError = msg;
        if (response.message.includes('تجاوز الحد')) {
          isRateLimited = true;
          loginAttempts = 5;
          remainingAttempts = 0;
          const timeMatch = response.message.match(/(\d+) دقيقة/);
          if (timeMatch) lockoutTimeRemaining = parseInt(timeMatch[1]);
        } else {
          loginAttempts++;
          remainingAttempts = Math.max(0, 5 - loginAttempts);
          if (loginAttempts >= 5) { isRateLimited = true; lockoutTimeRemaining = 5; }
        }
        throw new Error(msg);
      }
    });
  }

  function handleKeydown(e: CustomEvent<KeyboardEvent> | KeyboardEvent) {
    const key = e instanceof KeyboardEvent ? e.key : (e as CustomEvent<KeyboardEvent>).detail?.key ?? '';
    if (key === 'Enter') handleLogin();
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900" dir="rtl">
  <div class="w-full max-w-md">
  <AppCard elevated padding="lg">

    <!-- الشعار والعنوان -->
    <div class="text-center mb-8">
      <div class="w-16 h-16 bg-civil-blue rounded-full flex items-center justify-center mx-auto mb-4 shadow-lg" role="img" aria-label="شعار نظام GRPC">
        <svg class="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"/>
        </svg>
      </div>
      <h1 class="text-2xl font-bold text-gray-900 dark:text-white">GRPC</h1>
      <p class="text-gray-500 dark:text-gray-400 mt-1 text-sm">نظام إدارة مطاعم الحماية المدنية</p>
    </div>

    <!-- رسالة الخطأ -->
    {#if displayError}
      <div class="mb-4">
        <AppAlert intent="danger">{displayError}</AppAlert>
      </div>
    {/if}

    <!-- فتح تلقائي للمفتاح (ADR-0041 §11.4): مصدر keyring — إشعار + إدارة -->
    {#if appKeySource === 'keyring'}
      <div class="mb-4">
        <AppAlert intent="info">
          <p class="text-sm font-semibold">فتح تلقائي لمفتاح التطبيق مفعّل</p>
          <p class="text-xs mt-1">
            المفتاح محفوظ على هذا الجهاز. هذا لا يعني تسجيل الدخول — أدخل اسم
            المستخدم وكلمة المرور للمتابعة.
          </p>
          <button
            type="button"
            class="text-xs underline mt-2 text-civil-blue dark:text-civil-blue"
            on:click={() => push('/security')}
          >
            إدارة / نسيان المفتاح المحفوظ
          </button>
        </AppAlert>
      </div>
    {/if}

    <!-- مؤشر محاولات الدخول -->
    {#if loginAttempts > 0}
      <div class="mb-4">
        <AppAlert intent={isRateLimited ? 'danger' : 'warning'}>
          <div class="flex items-center justify-between">
            <span>محاولات تسجيل الدخول: {loginAttempts}/5</span>
            {#if remainingAttempts !== null}
              <span class="font-semibold">
                {remainingAttempts > 0 ? `${remainingAttempts} متبقية` : 'محظور'}
              </span>
            {/if}
          </div>
          <div class="mt-2 w-full bg-current/20 rounded-full h-1.5" role="progressbar" aria-valuenow={loginAttempts} aria-valuemin={0} aria-valuemax={5}>
            <div
              class="h-1.5 rounded-full transition-all duration-300 {loginAttempts >= 4 ? 'bg-red-500' : loginAttempts >= 2 ? 'bg-yellow-500' : 'bg-green-500'}"
              style="width: {(loginAttempts / 5) * 100}%"
            ></div>
          </div>
        </AppAlert>
      </div>
    {/if}

    <!-- التبويبات -->
    <div class="flex mb-4 border-b border-gray-200 dark:border-gray-700" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={authTab === 'password'}
        class="flex-1 pb-2 text-sm font-medium transition-colors border-b-2 {authTab === 'password' ? 'text-civil-blue border-civil-blue' : 'text-gray-500 dark:text-gray-400 border-transparent hover:text-gray-700 dark:hover:text-gray-300'}"
        on:click={() => { authTab = 'password'; authTabDirty = true; localError = ''; }}
      >
        كلمة المرور
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={authTab === 'adminkey'}
        disabled={!adminkeyAvailable}
        class="flex-1 pb-2 text-sm font-medium transition-colors border-b-2 disabled:cursor-not-allowed disabled:opacity-50 {authTab === 'adminkey' ? 'text-civil-blue border-civil-blue' : 'text-gray-500 dark:text-gray-400 border-transparent hover:text-gray-700 dark:hover:text-gray-300'}"
        on:click={() => { authTab = 'adminkey'; authTabDirty = true; localError = ''; }}
      >
        المفتاح الإداري
      </button>
    </div>

    <!-- نموذج الدخول بكلمة المرور -->
    {#if authTab === 'password'}
    <form class="space-y-4" on:submit|preventDefault={handleLogin} novalidate>
      <AppInput
        id="username"
        label="اسم المستخدم"
        type="text"
        bind:value={username}
        placeholder="أدخل اسم المستخدم"
        autocomplete="username"
        required
        disabled={$loginLoading || isRateLimited}
        on:keydown={handleKeydown}
      />

      <AppInput
        id="password"
        label="كلمة المرور"
        type="password"
        bind:value={password}
        placeholder="أدخل كلمة المرور"
        autocomplete="current-password"
        required
        disabled={$loginLoading || isRateLimited}
        on:keydown={handleKeydown}
      />

      <p class="text-xs text-gray-500 dark:text-gray-400 -mt-2 leading-relaxed">
        ملاحظة: حساب <code class="font-mono">admin</code> يستخدم <b>كلمة مرور المسؤول العام</b>
        — وليست كلمة مرور المفتاح الإداري. يتم تعيين كلمة مرور المسؤول العام من صفحة الإعدادات.
      </p>

      <AppButton
        type="submit"
        variant="primary"
        size="lg"
        fullWidth
        loading={$loginLoading}
        disabled={isRateLimited}
      >
        تسجيل الدخول
      </AppButton>
    </form>
    {/if}

    <!-- نموذج الدخول بالمفتاح الإداري (Challenge–Response) -->
    {#if authTab === 'adminkey'}
    <form class="space-y-4" on:submit|preventDefault={handleChallengeLogin} novalidate>
      <AppInput
        id="passphrase"
        label="كلمة مرور المفتاح الإداري"
        type="password"
        bind:value={passphrase}
        placeholder="أدخل كلمة مرور المفتاح"
        autocomplete="current-password"
        required
        disabled={$loginLoading || isRateLimited || !adminkeyAvailable}
      />

      <p class="text-xs text-gray-500 dark:text-gray-400 -mt-2 leading-relaxed">
        كلمة مرور المفتاح الإداري هي التي أنشأتها عند إصدار المفتاح على عقدة WILAYA — وهي
        <b>مختلفة</b> عن كلمة مرور الحساب (المسؤول العام).
      </p>

      <AppButton
        type="submit"
        variant="primary"
        size="lg"
        fullWidth
        loading={$loginLoading}
        disabled={isRateLimited || !adminkeyAvailable}
      >
        تسجيل الدخول بالمفتاح
      </AppButton>
    </form>
    {/if}

    <!-- استيراد حزمة التكوين -->
    {#if !isAppConfigured}
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700 space-y-3">
        <p class="text-sm text-gray-500 dark:text-gray-400 text-center">لم يتم تكوين العقدة بعد</p>
        <p class="text-xs text-gray-400 dark:text-gray-500 text-center">
          الخطوة 1: ثبّت شهادة WILAYA كمرساة ثقة قبل استيراد الحزمة (ADR-0045)
        </p>
        <p class="text-xs text-gray-400 dark:text-gray-500 text-center">
          تُعدّ عقدة WILAYA هوية الوحدة ضمن حزمة .unit وتضمّنها فيها، ويتضمن التصدير مادة هوية الوحدة. المفتاح الخاص محمي ولا يُعرَض للمستخدم، وتُفعَّل هوية الوحدة على هذه العقدة بعد التحقق من الحزمة.
        </p>
        <AppButton
          variant="secondary"
          size="lg"
          fullWidth
          loading={$bootstrapLoading}
          on:click={handleInstallWilayaCert}
        >
          الخطوة 1: تثبيت شهادة WILAYA (مرساة الثقة)
        </AppButton>
        <AppButton
          variant="secondary"
          size="lg"
          fullWidth
          loading={$importLoading}
          disabled={!unitAnchorInstalled}
          on:click={handleImportPackage}
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
          </svg>
          الخطوة 2: استيراد حزمة التكوين (.unit)
        </AppButton>
      </div>
    {/if}

    <!-- تهيئة الهوية (B5: offline Root bootstrap) -->
    {#if wilayaBootstrapActive}
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700 space-y-4">
        <div class="flex items-center justify-between">
          <p class="text-sm font-medium text-gray-700 dark:text-gray-300">تهيئة الهوية</p>
          <button
            type="button"
            class="text-xs text-civil-blue hover:underline"
            on:click={refreshIdentityStatus}
          >
            تحديث
          </button>
        </div>
        <AppAlert intent="info">
          <div class="space-y-1">
            <p class="text-sm font-semibold">{bootstrapStatusLabel(identityState)}</p>
            <p class="text-xs">التوقيع يتم على جهاز المرجع (Root) دون اتصال — المفتاح السري لا يغادر العقدة.</p>
          </div>
        </AppAlert>

        {#if identityState === 'UNINITIALIZED'}
          <AppButton
            variant="secondary"
            size="lg"
            fullWidth
            loading={$bootstrapLoading}
            on:click={handleBeginWilaya}
          >
            تصدير طلب توقيع العقدة (CSR)
          </AppButton>
        {/if}

        {#if identityState === 'WAITING_FOR_ROOT_CERTIFICATE'}
          <AppButton
            variant="secondary"
            size="lg"
            fullWidth
            loading={$bootstrapLoading}
            on:click={handleFinalizeWilaya}
          >
            استيراد الشهادة الموقعة من المرجع
          </AppButton>
        {/if}

        {#if identityState === 'WILAYA_ACTIVE'}
          <div class="space-y-3">
            <AppInput
              id="bootstrap-username"
              label="اسم مستخدم المدير"
              type="text"
              bind:value={bootstrapUsername}
              disabled={$bootstrapLoading}
            />
            <AppInput
              id="bootstrap-passphrase"
              label="كلمة مرور المفتاح الإداري"
              type="password"
              bind:value={bootstrapPassphrase}
              placeholder="أنشئ كلمة مرور تحمي المفتاح"
              disabled={$bootstrapLoading}
            />
            <AppButton
              variant="primary"
              size="lg"
              fullWidth
              loading={$bootstrapLoading}
              on:click={handleIssueAdminKey}
            >
              إصدار المفتاح الإداري الأول
            </AppButton>
          </div>
        {/if}
      </div>
    {/if}

    <!-- تهيئة هوية الوحدة (RFC §3.12 / B6-A: strict two-step, trust anchor غير مضمّنة) -->
    {#if unitBootstrapActive}
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700 space-y-4">
        <div class="flex items-center justify-between">
          <p class="text-sm font-medium text-gray-700 dark:text-gray-300">تهيئة هوية الوحدة</p>
          <button
            type="button"
            class="text-xs text-civil-blue hover:underline"
            on:click={refreshIdentityStatus}
          >
            تحديث
          </button>
        </div>
        <AppAlert intent="info">
          <div class="space-y-1">
            <p class="text-sm font-semibold">{bootstrapStatusLabel(identityState)}</p>
            <p class="text-xs">توقّع عقدة WILAYA شهادة الوحدة وتُعدّ هويتها ضمن حزمة .unit. المفتاح الخاص محمي ولا يُعرَض للمستخدم، وشهادة WILAYA (مرساة الثقة) تُثبَّت في خطوة مستقلة.</p>
          </div>
        </AppAlert>

        {#if identityState === 'UNINITIALIZED'}
          <AppButton
            variant="secondary"
            size="lg"
            fullWidth
            loading={$bootstrapLoading}
            on:click={handleBeginUnit}
          >
            تصدير طلب توقيع الوحدة (CSR)
          </AppButton>
        {/if}

        {#if identityState === 'UNIT_WAITING_FOR_CERTIFICATE'}
          <div class="space-y-3">
            <AppButton
              variant="secondary"
              size="lg"
              fullWidth
              loading={$bootstrapLoading}
              on:click={handleInstallWilayaCert}
            >
              الخطوة 1: تثبيت شهادة WILAYA (مرساة الثقة)
            </AppButton>
            <AppButton
              variant="secondary"
              size="lg"
              fullWidth
              loading={$bootstrapLoading}
              on:click={handleFinalizeUnit}
            >
              الخطوة 2: استيراد شهادة الوحدة الموقعة
            </AppButton>
          </div>
        {/if}

        {#if identityState === 'UNIT_ACTIVE'}
          <AppAlert intent="success">
            <p class="text-sm font-semibold">هوية الوحدة مفعلة — يمكنك تسجيل الدخول.</p>
          </AppAlert>
        {/if}
      </div>
    {/if}
  </AppCard>
  </div>
</div>
