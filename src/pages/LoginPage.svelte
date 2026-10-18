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
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const loginOp = createOperation({ scope });
  const loginLoading = loginOp.loading;
  const loginError = loginOp.error;

  const importOp = createOperationGuard({ scope });
  const importLoading = importOp.loading;

  // @category TransientState — ADR-0052: the operator identity is chosen from
  // a fixed set (UNIT) or pinned (WILAYA); free-text entry is gone.
  let username = 'user';
  // @category TransientState
  let password = '';
  // @category TransientState
  let localError = '';
  // @category ProjectionState — fail-closed readiness latch: stays false until
  // every required initial status read (security / configuration / settings /
  // identity) has resolved successfully. Login controls never render before
  // this is true, so there is no pre-read flash.
  let statusLoaded = false;
  // @category ProjectionState
  let isAppConfigured = false;
  // @category ProjectionState — `get_settings().configured`. This is a distinct
  // read from `is_configured()`; both must agree (and neither may be optimized
  // away) before a node is treated as genuinely uninitialized.
  let settingsConfigured = false;
  // @category ProjectionState
  let isUnitNode = false;
  // @category UiState — transient first-run WILAYA/UNIT entry selection. It is
  // UI-only: never persisted and never written to backend settings. It is only
  // meaningful while the node is genuinely uninitialized.
  let setupTab: 'WILAYA' | 'UNIT' = 'UNIT';

  // ADR-0052: WILAYA nodes authenticate exclusively as `admin`; the identity
  // field state mirrors the rendered (readonly) field at all times.
  $: if (!isUnitNode) username = 'admin';
  // The admin-key tab is WILAYA-only. Whenever the node is identified as UNIT
  // (e.g. after settings load), silently fall back to the password tab so the
  // hidden tab cannot remain "selected" with no corresponding UI.
  $: if (isUnitNode && authTab === 'adminkey') authTab = 'password';

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
  //
  // `WILAYA_ACTIVE` is deliberately NOT included: at that state no ACTIVE ADMIN
  // credential and no `.adminkey` exist yet, so the challenge path cannot
  // succeed. Presenting it would be a non-functional affordance (the backend
  // fails closed regardless).
  // @category UiState
  $: adminkeyAvailable =
    identityState === 'ADMIN_PROVISIONED' ||
    identityState === 'READY';
  // UNIT-only identity/bootstrap states. They can arise ONLY on the UNIT chain
  // (both require a configured UNIT node key), so they are safe to branch on for
  // presentation isolation and must never surface WILAYA-only setup UI.
  // @category UiState
  $: unitOnlyState =
    identityState === 'UNIT_WAITING_FOR_CERTIFICATE' ||
    identityState === 'UNIT_ACTIVE';
  // @category UiState
  $: wilayaBootstrapActive =
    !isUnitNode &&
    !unitOnlyState &&
    (identityState === 'UNINITIALIZED' ||
      identityState === 'WAITING_FOR_ROOT_CERTIFICATE' ||
      // `WILAYA_ACTIVE` is deliberately kept for BOTH paths. The identity
      // projection cannot distinguish a finalized WILAYA node (still awaiting
      // first-admin-key issuance) from a UNIT node that installed the WILAYA
      // trust anchor (still awaiting `.unit` import): both report the same
      // state with no node key exposed. Hiding the WILAYA continuation here
      // would strand the WILAYA operator; the backend stays fail-closed for the
      // UNIT case (`issue_first_admin_key` requires a node key).
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

  // ADR-0050 / first-admin ceremony: a genuinely uninitialized node owns no
  // node key, is not configured by either authoritative read, and reports the
  // untouched `UNINITIALIZED` bootstrap chain.
  // @category UiState
  $: genuinelyUninitialized =
    statusLoaded &&
    !isAppConfigured &&
    !settingsConfigured &&
    identityState === 'UNINITIALIZED';

  // Login visibility is a PRESENTATION gate, never an authorization decision —
  // the backend remains authoritative and fail-closed. It must not gate on
  // `isAppConfigured` alone: WILAYA configuration is POST-auth, so requiring it
  // would deadlock the first-admin ceremony. `statusLoaded` makes it fail closed
  // (hidden) until every status read succeeds.
  // @category UiState
  $: loginEligible =
    statusLoaded &&
    (isAppConfigured ||
      identityState === 'ADMIN_PROVISIONED' ||
      identityState === 'READY' ||
      identityState === 'UNIT_ACTIVE');

  // @category UiState — classification: an unconfigured `WILAYA_ACTIVE` node is
  // AMBIGUOUS. The identity projection cannot distinguish a finalized WILAYA
  // node awaiting its first administrator key from a UNIT node that installed
  // the WILAYA trust anchor and still awaits its `.unit` package. The path is
  // never inferred from this state; instead the selector stays available and the
  // operator decides which single continuation to view.
  $: ambiguousWilayaActive =
    statusLoaded &&
    !isAppConfigured &&
    !settingsConfigured &&
    identityState === 'WILAYA_ACTIVE';

  // @category UiState — selection surface: the selector is offered in exactly
  // the two states where the operator must choose a continuation (a fresh node,
  // or the ambiguous `WILAYA_ACTIVE` above). In every other state the identity
  // projection alone decides which continuation is drawn.
  $: showSetupSelector = genuinelyUninitialized || ambiguousWilayaActive;

  // @category UiState — rendering exclusivity: the WILAYA-only continuation
  // states are unambiguous and always draw the WILAYA section; the selector
  // states draw ONLY the selected continuation, so the WILAYA first-admin form
  // and the UNIT entry can never be visible at the same time. Never shown on a
  // configured node.
  $: showWilayaSetup =
    !isAppConfigured &&
    wilayaBootstrapActive &&
    (!showSetupSelector || setupTab === 'WILAYA');
  // @category UiState — rendering exclusivity (UNIT entry): only while the UNIT
  // continuation is selected in a selector state. Withheld from every
  // unambiguous WILAYA continuation state and from any configured node.
  $: showUnitSetup =
    !isAppConfigured && showSetupSelector && setupTab === 'UNIT';

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

  async function refreshNodeType(): Promise<boolean> {
    let ok = true;
    try {
      const settings = await getSettings();
      settingsConfigured = settings.configured;
      isUnitNode = settings.configured && settings.node_type === 'UNIT';
    } catch {
      // Fail closed: an unreadable settings projection is never treated as a
      // configured node. `settingsConfigured` gates the setup selector, so a
      // read failure simply keeps the selector/phasing conservative.
      settingsConfigured = false;
      isUnitNode = false;
      ok = false;
    }
    // ADR-0052: seed the identity field per node class — the canonical UNIT
    // operator by default, or the pinned fleet admin on WILAYA.
    username = isUnitNode ? 'user' : 'admin';
    return ok;
  }

  async function refreshIdentityStatus() {
    try {
      identityState = await getIdentityStatus();
    } catch (e) {
      localError = 'تعذر قراءة حالة الهوية: ' + formatErrorMessage(e);
    }
  }

  /**
   * Re-reads the authoritative status projections after a provisioning command
   * rejected, without retrying the command or regenerating/replacing identity.
   *
   * A rejected command is NOT proof of no side effects: `begin_wilaya_provision`
   * and `begin_unit_provision` persist the node signing key BEFORE writing the
   * CSR file, so the command can reject while `get_identity_status` has already
   * moved off `UNINITIALIZED`. Trusting the stale projection would leave the
   * first-run selector (and the login gate) presenting a state the backend no
   * longer holds.
   *
   * The ORIGINAL operation error is always preserved — a failed refresh never
   * masks it. When either read fails, `statusLoaded` is reset to `false`, which
   * re-arms every fail-closed gate (setup selector + login controls) until the
   * page is reloaded and the initial reads succeed again; the uncertainty is
   * appended to the original error instead.
   */
  async function refreshAfterProvisionFailure(operationError: string): Promise<void> {
    const settingsOk = await refreshNodeType();
    let identityOk = true;
    try {
      identityState = await getIdentityStatus();
    } catch {
      identityOk = false;
    }
    if (settingsOk && identityOk) {
      localError = operationError;
      return;
    }
    localError =
      operationError + ' — تعذر تأكيد حالة العقدة بعد الفشل؛ أعد فتح الصفحة للتحقق.';
    statusLoaded = false;
  }

  onMount(async () => {
    // ADR-0041: if the app key does not resolve (locked store / unprovisioned),
    // route to the Security Setup / Unlock page — the DB is deferred and no
    // command below can run yet.
    //
    // Every required initial status read is tracked; a single failure leaves
    // `statusLoaded` false (fail closed) so the login controls stay hidden.
    let readsOk = true;

    try {
      const security = await getSecurityStatus();
      appKeySource = security.source;
      if (security.requires_action) {
        push('/security');
        return;
      }
    } catch (e) {
      console.error('Failed to read security status:', e);
      readsOk = false;
    }

    try {
      isAppConfigured = await isConfigured();
    } catch (e) {
      console.error('Failed to read configuration state:', e);
      isAppConfigured = false;
      readsOk = false;
    }

    if (!(await refreshNodeType())) {
      readsOk = false;
    }

    try {
      identityState = await getIdentityStatus();
    } catch (e) {
      console.error('Failed to read identity status:', e);
      localError = 'تعذر قراءة حالة الهوية: ' + formatErrorMessage(e);
      readsOk = false;
    }

    // Fail closed: partial success never marks initialization complete.
    statusLoaded = readsOk;
    // The `grpc_setup_node_type` session value is a transient UI HINT only. It
    // is applied solely in the two states that expose the selector (a genuinely
    // uninitialized node, or the ambiguous unconfigured `WILAYA_ACTIVE`), where
    // it preselects a tab WITHOUT suppressing the other continuation — the
    // selector always keeps both reachable. It must never override a definitive
    // identity state: `WAITING_FOR_ROOT_CERTIFICATE` and the UNIT-only states
    // are decided by the identity projection alone.
    if (
      readsOk &&
      !isAppConfigured &&
      !settingsConfigured &&
      (identityState === 'UNINITIALIZED' || identityState === 'WILAYA_ACTIVE')
    ) {
      const savedNodeType =
        typeof sessionStorage !== 'undefined'
          ? sessionStorage.getItem('grpc_setup_node_type')
          : null;
      if (savedNodeType === 'WILAYA' || savedNodeType === 'UNIT') {
        setupTab = savedNodeType;
      }
    }
    if (!readsOk && !localError) {
      localError = 'تعذر قراءة حالة تهيئة العقدة. تحقق من النظام ثم أعد المحاولة.';
    }

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
        await refreshAfterProvisionFailure(
          'خطأ في استيراد الحزمة: ' + formatErrorMessage(e)
        );
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
        await refreshAfterProvisionFailure(
          'خطأ في بدء تهيئة الهوية: ' + formatErrorMessage(e)
        );
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
        await refreshAfterProvisionFailure(
          'خطأ في تفعيل الشهادة: ' + formatErrorMessage(e)
        );
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
        await refreshAfterProvisionFailure(
          'خطأ في إصدار المفتاح الإداري: ' + formatErrorMessage(e)
        );
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
        await refreshAfterProvisionFailure(
          'خطأ في بدء تهيئة الوحدة: ' + formatErrorMessage(e)
        );
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
        await refreshAfterProvisionFailure(
          'خطأ في تثبيت شهادة WILAYA: ' + formatErrorMessage(e)
        );
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
        await refreshAfterProvisionFailure(
          'خطأ في تفعيل هوية الوحدة: ' + formatErrorMessage(e)
        );
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
    // ADR-0063 §5/D32: `must_change_password` is a backend-owned persisted
    // fact. When it is active the operator is routed to the self-change
    // surface instead of the normal node home; every other login keeps its
    // existing routing.
    if (response.user?.must_change_password) {
      push('/settings');
      return;
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

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900" dir="rtl" data-testid="login-page">
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

    <!--
      Login controls are gated on `loginEligible` — a PRESENTATION gate only;
      the backend remains authoritative and fail-closed. They never render
      until every required status read succeeds (no pre-read flash) and stay
      hidden on an unconfigured node that is not yet authenticatable.
    -->
    {#if loginEligible}
    <!-- التبويبات — المفتاح الإداري خاص بـ WILAYA فقط؛ يُخفى كلياً على عقد UNIT -->
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
      {#if !isUnitNode}
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
      {/if}
    </div>

    <!-- نموذج الدخول بكلمة المرور -->
    {#if authTab === 'password'}
    <form class="space-y-4" on:submit|preventDefault={handleLogin} novalidate>
      {#if isUnitNode}
        <AppSelect id="username" label="المستخدم" bind:value={username} disabled={$loginLoading || isRateLimited}>
          <option value="user">مشغّل الوحدة (user)</option>
          <option value="admin">المسؤول العام (admin)</option>
        </AppSelect>
      {:else}
        <AppInput
          id="username"
          label="المسؤول العام (admin)"
          type="text"
          value="admin"
          readonly
          autocomplete="username"
          on:keydown={handleKeydown}
        />
      {/if}

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
        — وليست كلمة مرور المفتاح الإداري. يتم تعيين كلمة مرور المسؤول العام من صفحة الإعدادات،
        ومستخدم الوحدة ثابت باسم <code class="font-mono" dir="ltr">user</code>.
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
    {/if}

    <!--
      Continuation selector (WILAYA / UNIT). Presentation-only: the choice is
      transient in-memory state and never reaches backend settings or the
      database. It is offered while the node is genuinely uninitialized
      (`identityState === 'UNINITIALIZED'`) and while an unconfigured node is in
      the AMBIGUOUS `WILAYA_ACTIVE` state — where the identity projection cannot
      say whether the pending step is WILAYA first-admin-key issuance or a UNIT
      `.unit` import. It selects which SINGLE continuation renders; it never
      changes an established node type (the backend refuses any restart /
      identity change) and never proves the persisted provisioning path.
    -->
    {#if showSetupSelector}
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700 space-y-3">
        <p class="text-sm font-medium text-gray-700 dark:text-gray-300 text-center">
          اختر نوع العقدة
        </p>
        <div class="flex gap-2" role="group" aria-label="نوع العقدة">
          <button
            type="button"
            aria-pressed={setupTab === 'WILAYA'}
            class="flex-1 rounded-lg border px-3 py-2 text-sm font-medium transition-colors {setupTab === 'WILAYA' ? 'border-civil-blue bg-civil-blue/10 text-civil-blue font-bold shadow-sm' : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800'}"
            on:click={() => {
              setupTab = 'WILAYA';
              if (typeof sessionStorage !== 'undefined') sessionStorage.setItem('grpc_setup_node_type', 'WILAYA');
            }}
          >
            ولاية (WILAYA)
          </button>
          <button
            type="button"
            aria-pressed={setupTab === 'UNIT'}
            class="flex-1 rounded-lg border px-3 py-2 text-sm font-medium transition-colors {setupTab === 'UNIT' ? 'border-civil-blue bg-civil-blue/10 text-civil-blue font-bold shadow-sm' : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800'}"
            on:click={() => {
              setupTab = 'UNIT';
              if (typeof sessionStorage !== 'undefined') sessionStorage.setItem('grpc_setup_node_type', 'UNIT');
            }}
          >
            وحدة (UNIT)
          </button>
        </div>
      </div>
    {/if}

    <!--
      Ambiguous unconfigured `WILAYA_ACTIVE`: the identity projection cannot
      distinguish a finalized WILAYA node (first-admin key pending) from a UNIT
      anchor-first node (`.unit` import pending). The selector above lets the
      operator choose, and exactly ONE continuation is drawn below for the
      current selection — never both. This note never selects a path.
    -->
    {#if ambiguousWilayaActive}
      <div class="mt-6">
        <AppAlert intent="info">
          <p class="text-xs leading-relaxed">
            حالة التهيئة غير حاسمة: اختر المسار المناسب من المحدّد أعلاه. إن كانت
            هذه عقدة ولاية (WILAYA) فاختر «ولاية» لإكمال إصدار المفتاح الإداري الأول؛
            وإن كانت وحدة (UNIT) فاختر «وحدة» لتثبيت مرساة الثقة ثم استيراد حزمة
            <code class="font-mono">.unit</code>. يُعرض مسار واحد فقط في كل مرة،
            ويبقى الخادم المرجع النهائي للمسار الصحيح.
          </p>
        </AppAlert>
      </div>
    {/if}

    <!-- استيراد حزمة التكوين — UNIT-path entry (trust-anchor install + `.unit`
         import). Rendered only while the UNIT continuation is selected in a
         selector state (genuinely uninitialized, or ambiguous `WILAYA_ACTIVE`);
         never shown for WILAYA-chain continuation states
         (`WAITING_FOR_ROOT_CERTIFICATE`, `ADMIN_PROVISIONED`, `READY`) nor on a
         configured node. See the `showUnitSetup` derivation. -->
    {#if showUnitSetup}
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
    {#if showWilayaSetup}
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
