<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { openFile, saveFile, getAppWindow, createLogicalSize } from '../lib/tauri';
  import {
    importUnitNodePackage,
    login,
    getSettings,
    isConfigured,
    getIdentityStatus,
    beginWilayaProvision,
    finalizeWilayaProvision,
    issueFirstAdminKey,
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
  let loginAttempts = 0;
  // @category ProjectionState
  let remainingAttempts: number | null = null;
  // @category ProjectionState
  let lockoutTimeRemaining: number | null = null;
  // @category ProjectionState
  let isRateLimited = false;

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

  // B6-A (ADR-0038): the legacy password path is available only while no ACTIVE
  // ADMIN identity exists. Once the admin identity is present (ADMIN_PROVISIONED
  // or READY) Challenge–Response is the mandatory login path — the password tab
  // is hidden and the admin-key tab becomes the default.
  // @category UiState
  $: passwordLoginAvailable =
    identityState !== 'ADMIN_PROVISIONED' && identityState !== 'READY';
  // @category UiState
  $: effectiveAuthTab = passwordLoginAvailable ? authTab : 'adminkey';
  // @category UiState
  $: adminkeyAvailable =
    identityState === 'WILAYA_ACTIVE' ||
    identityState === 'ADMIN_PROVISIONED' ||
    identityState === 'READY';
  // @category UiState
  $: bootstrapActive =
    identityState === 'UNINITIALIZED' ||
    identityState === 'WAITING_FOR_ROOT_CERTIFICATE' ||
    identityState === 'WILAYA_ACTIVE';

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
    try {
      isAppConfigured = await isConfigured();
    } catch (e) {
      isAppConfigured = false;
    }
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
        // The backend gate closed the password path (ACTIVE ADMIN identity).
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
      {#if passwordLoginAvailable}
      <button
        type="button"
        role="tab"
        aria-selected={effectiveAuthTab === 'password'}
        class="flex-1 pb-2 text-sm font-medium transition-colors border-b-2 {effectiveAuthTab === 'password' ? 'text-civil-blue border-civil-blue' : 'text-gray-500 dark:text-gray-400 border-transparent hover:text-gray-700 dark:hover:text-gray-300'}"
        on:click={() => { authTab = 'password'; authTabDirty = true; localError = ''; }}
      >
        كلمة المرور
      </button>
      {/if}
      <button
        type="button"
        role="tab"
        aria-selected={effectiveAuthTab === 'adminkey'}
        disabled={!adminkeyAvailable}
        class="flex-1 pb-2 text-sm font-medium transition-colors border-b-2 disabled:cursor-not-allowed disabled:opacity-50 {effectiveAuthTab === 'adminkey' ? 'text-civil-blue border-civil-blue' : 'text-gray-500 dark:text-gray-400 border-transparent hover:text-gray-700 dark:hover:text-gray-300'}"
        on:click={() => { authTab = 'adminkey'; authTabDirty = true; localError = ''; }}
      >
        المفتاح الإداري
      </button>
    </div>

    <!-- نموذج الدخول بكلمة المرور -->
    {#if effectiveAuthTab === 'password'}
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
    {#if effectiveAuthTab === 'adminkey'}
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
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700">
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-3 text-center">لم يتم تكوين العقدة بعد</p>
        <AppButton
          variant="secondary"
          size="lg"
          fullWidth
          loading={$importLoading}
          on:click={handleImportPackage}
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
          </svg>
          استيراد حزمة التكوين (.unit)
        </AppButton>
      </div>
    {/if}

    <!-- تهيئة الهوية (B5: offline Root bootstrap) -->
    {#if bootstrapActive}
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
  </AppCard>
  </div>
</div>
