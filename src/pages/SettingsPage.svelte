<script lang="ts">
  // الإعدادات — SEC-014 Phase 4 + SEC-021: the production home for the
  // post-provisioning Admin credential and Admin-Only account synchronization
  // (ADR-0045 / ADR-0051, Decision D1).
  //
  // Authenticated page only: every action here requires a CurrentSession and
  // the backend remains the sole authorization authority:
  //   - setFleetAdminPassword     → ManageAccountSync (WILAYA + AdminOnly)
  //   - exportAdminAccessPackage  → ExportAdminAccessPackage (WILAYA + AdminOnly)
  //                                 — fleet-wide, NO unit selector exists
  //   - importAdminAccessPackage  → AuthenticatedOnly + FirstImportBootstrap
  //                                 (UNIT User) / AdminOnly re-import (UNIT Admin)
  // The admin_access package synchronizes ONLY the canonical `admin` account;
  // the UNIT operator account provisioned by `.unit` is never touched. This
  // page never hashes, stores, or renders secrets, never creates a session,
  // and never performs automatic navigation after an import.
  import { onMount, onDestroy } from 'svelte';
  import {
    getSettings,
    setFleetAdminPassword,
    exportAdminAccessPackage,
    importAdminAccessPackage,
    changeOwnPassword,
    resetUnitUserPassword,
  } from '../lib/contracts';
  import { openFile, saveFile } from '../lib/tauri';
  import type { Settings } from '../lib/types';
  import { push } from 'svelte-spa-router';
  import { currentUser as userStore, refreshCurrentUser } from '../lib/session';
  import Layout from '../components/Layout.svelte';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';
  import { formatErrorMessage } from '../lib/errors';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const initialOp = createOperation({ scope });
  const loading = initialOp.loading;

  const accountOp = createOperationGuard({ scope });
  const operationLoading = accountOp.loading;

  const b8ImportOp = createOperationGuard({ scope });
  const b8ImportLoading = b8ImportOp.loading;

  const selfChangeOp = createOperationGuard({ scope });
  const selfChangeLoading = selfChangeOp.loading;

  const unitResetOp = createOperationGuard({ scope });
  const unitResetLoading = unitResetOp.loading;

  // @category SessionState
  $: user = $userStore;
  // @category UiState
  $: isAdmin = user?.role === 'Admin';
  // @category ProjectionState — observed, never owned: ADR-0063 §5 states the
  // forced credential state is a backend-persisted fact read through the
  // `User` projection (owner: sync.contract.ts).
  $: mustChangePassword = user?.must_change_password === true;

  // @category ProjectionState
  let settings: Settings | null = null;
  // @category UiState
  let nodeType: 'WILAYA' | 'UNIT' | null = null;
  // @category UiState — true while redirecting an unauthorized visitor away
  let accessDenied = false;

  // ── WILAYA: fleet admin credential (أمان الحسابات) ────────────────────────
  // Transient only: cleared after submission, never rendered back, never
  // persisted, never logged. It is a DIFFERENT secret from the Admin Key
  // passphrase.
  // @category TransientState
  let fleetPassword = '';
  // @category TransientState
  let fleetConfirmPassword = '';
  // @category TransientState
  let fleetError = '';
  // @category TransientState
  let fleetSuccess = '';

  // ── WILAYA: admin_access export (مزامنة حساب المدير العام) ────────────────
  // SEC-033: fleet-level export — the backend enumerates the authoritative
  // UNIT target set and emits one signed artifact per target; the renderer
  // expresses fleet intent only (no UNIT selector exists here).
  // @category TransientState
  let b8ExportProgress = '';

  // ── UNIT: B8 import (مزامنة الحسابات) ─────────────────────────────────────
  // @category TransientState
  let b8ImportSuccess = '';
  // @category TransientState
  let b8ImportError = '';

  // ── Forced self-change (ADR-0063 §6) ──────────────────────────────────────
  // Transient only: never persisted, never rendered back after submission,
  // never logged. The backend is the sole authority for current-password
  // verification, policy validation, reuse rejection and forced-state clearing;
  // the checks below only mirror its rules for immediate UX feedback.
  // @category TransientState
  let selfChangeCurrent = '';
  // @category TransientState
  let selfChangeNew = '';
  // @category TransientState
  let selfChangeConfirm = '';
  // @category TransientState
  let selfChangeError = '';
  // @category TransientState
  let selfChangeSuccess = '';

  // ── UNIT admin reset of the canonical local operator (ADR-0063 §7.1) ──────
  // Transient only: never persisted, never rendered back after submission,
  // never logged. No target selector exists — the backend derives the operator
  // row server-side from the local UNIT code and is the sole authority for
  // policy validation and forced-state setting.
  // @category TransientState
  let resetUnitPassword = '';
  // @category TransientState
  let resetUnitConfirm = '';
  // @category TransientState
  let resetUnitError = '';
  // @category TransientState
  let resetUnitSuccess = '';

  onMount(async () => {
    await initialOp.run(async () => {
      if (!$userStore) {
        accessDenied = true;
        push('/login');
        return;
      }
      settings = await getSettings();
      nodeType = settings?.node_type ?? null;
      if (nodeType === 'WILAYA' && !isAdmin) {
        // WILAYA User has no actionable settings section.
        accessDenied = true;
        push('/wilaya');
        return;
      }
    });
  });

  // WILAYA Admin initializes/changes the fleet admin credential. The backend
  // command is the sole authority (ManageAccountSync → Wilaya + AdminOnly);
  // this form only relays the new secret — validation rules are relayed for
  // UX, the backend enforces its own.
  async function handleSetFleetPassword() {
    if (fleetPassword.length < 8) {
      fleetError = 'كلمة المرور قصيرة جداً — يجب أن تكون 8 أحرف على الأقل';
      return;
    }
    if (fleetPassword !== fleetConfirmPassword) {
      fleetError = 'كلمتا المرور غير متطابقتين';
      return;
    }
    await accountOp.guard(async () => {
      try {
        fleetError = '';
        fleetSuccess = '';
        await setFleetAdminPassword(fleetPassword);
        fleetPassword = '';
        fleetConfirmPassword = '';
        fleetSuccess =
          'تم تعيين كلمة مرور المسؤول العام. يمكن الآن تسجيل الدخول بكلمة المرور وتصدير حزم الحسابات (B8).';
      } catch (e) {
        fleetError = formatErrorMessage(e);
      }
    });
  }

  // WILAYA-only signed/encrypted admin_access export (ADR-0051 + SEC-033).
  // The backend enumerates the authoritative UNIT targets and builds, signs,
  // and encrypts ONE package per target (fail-closed while the fleet password
  // is unset or no UNIT is registered). Package contents never enter the
  // frontend.
  async function handleExportAdminAccess() {
    await accountOp.guard(async () => {
      try {
        const selected = await saveFile({
          defaultPath: `grpc-admin-access.sync`,
          filters: [{ name: 'حزمة حساب المدير العام', extensions: ['sync'] }],
        });
        if (!selected) return;
        b8ExportProgress = 'تصدير حزم حساب المدير العام (admin)...';
        fleetError = '';
        fleetSuccess = '';
        const result = await exportAdminAccessPackage(selected as string);
        fleetSuccess =
          `تم تصدير حزمة حساب المدير العام (${result.record_count} سجلات) إلى جميع الوحدات المُسجَّلة — حزمة موقّعة لكل وحدة. لا تُعدّل حساب مشغّل الوحدة إطلاقًا.`;
        b8ExportProgress = '';
      } catch (e) {
        fleetError = formatErrorMessage(e);
        b8ExportProgress = '';
      }
    });
  }

  // UNIT-side admin_access import (first import via FirstImportBootstrap for
  // a User session; authorized re-import via the AdminOnly path for an Admin
  // session — the backend decides). The package synchronizes ONLY the
  // canonical `admin` account: the local operator account provisioned by
  // `.unit` keeps its username and password exactly as provisioned. Success
  // refreshes local account state only: no session is created, no automatic
  // navigation occurs, and the current session stays untouched.
  async function handleImportAdminAccess() {
    await b8ImportOp.guard(async () => {
      try {
        b8ImportError = '';
        b8ImportSuccess = '';
        const selected = await openFile({
          multiple: false,
          filters: [{ name: 'حزمة حساب المدير العام', extensions: ['sync'] }],
        });
        if (!selected) return;
        await importAdminAccessPackage(selected as string);
        b8ImportSuccess =
          'تمت مزامنة حساب المدير العام بنجاح. يمكنك الآن تسجيل الخروج ثم تسجيل الدخول باسم admin باستخدام كلمة مرور المسؤول العام المعينة على عقدة WILAYA. حساب مشغّل الوحدة الخاص بك لم يُمَسّ — استمر باستخدام اسم المستخدم وكلمة المرور المُوفرَّين عبر ملف .unit.';
      } catch (e) {
        b8ImportError = 'خطأ في استيراد حزمة حساب المدير العام: ' + formatErrorMessage(e);
      }
    });
  }

  // ADR-0063 §6/D33 — canonical local UNIT operator self password change.
  // The backend derives the target row from the authenticated session and
  // rejects wrong current passwords, policy violations, reuse, and any
  // non-canonical actor. On success the authoritative projection is re-read
  // from the backend and published to the session store: the UI never flips
  // its own copy of `must_change_password`.
  async function handleChangeOwnPassword() {
    if (!selfChangeCurrent) {
      selfChangeError = 'أدخل كلمة المرور الحالية';
      return;
    }
    if (selfChangeNew.length < 8) {
      selfChangeError = 'كلمة المرور يجب أن تكون 8 أحرف على الأقل';
      return;
    }
    if (!/[A-Z]/.test(selfChangeNew) || !/[a-z]/.test(selfChangeNew) || !/[0-9]/.test(selfChangeNew)) {
      selfChangeError = 'كلمة المرور يجب أن تحتوي على حرف كبير وحرف صغير ورقم';
      return;
    }
    if (selfChangeNew !== selfChangeConfirm) {
      selfChangeError = 'كلمتا المرور غير متطابقتين';
      return;
    }
    await selfChangeOp.guard(async () => {
      try {
        selfChangeError = '';
        selfChangeSuccess = '';
        await changeOwnPassword(selfChangeCurrent, selfChangeNew);
        selfChangeCurrent = '';
        selfChangeNew = '';
        selfChangeConfirm = '';
        await refreshCurrentUser();
        selfChangeSuccess = 'تم تغيير كلمة المرور بنجاح. يمكنك الآن متابعة العمل بالتطبيق.';
      } catch (e) {
        selfChangeError = formatErrorMessage(e);
      }
    });
  }

  // ADR-0063 §7.1/D6 — local UNIT admin reset of the canonical local operator
  // `user`. The backend derives the target server-side from the local UNIT
  // code (no username/unit selector exists) and is the sole authority for
  // policy validation, node-bound hashing, forced-state setting and audit.
  // The reset target is NOT the current session, so no projection refresh and
  // no session/navigation change occurs here.
  async function handleResetUnitUserPassword() {
    if (!resetUnitPassword) {
      resetUnitError = 'أدخل كلمة مرور مؤقتة للمشغّل';
      return;
    }
    if (resetUnitPassword.length < 8) {
      resetUnitError = 'كلمة المرور يجب أن تكون 8 أحرف على الأقل';
      return;
    }
    if (!/[A-Z]/.test(resetUnitPassword) || !/[a-z]/.test(resetUnitPassword) || !/[0-9]/.test(resetUnitPassword)) {
      resetUnitError = 'كلمة المرور يجب أن تحتوي على حرف كبير وحرف صغير ورقم';
      return;
    }
    if (resetUnitPassword !== resetUnitConfirm) {
      resetUnitError = 'كلمتا المرور غير متطابقتين';
      return;
    }
    await unitResetOp.guard(async () => {
      try {
        resetUnitError = '';
        resetUnitSuccess = '';
        await resetUnitUserPassword(resetUnitPassword);
        resetUnitPassword = '';
        resetUnitConfirm = '';
        resetUnitSuccess =
          'تمت إعادة تعيين كلمة مرور مشغّل الوحدة. سيُطلب منه تغييرها إلزامياً عند تسجيل الدخول التالي.';
      } catch (e) {
        resetUnitError = formatErrorMessage(e);
      }
    });
  }
</script>

<Layout {nodeType} title="الإعدادات" subtitle="إدارة الحسابات ومزامنتها">
  <div dir="rtl">
    <AppPageHeader title="الإعدادات" subtitle="إدارة الحسابات ومزامنتها" />

    {#if $loading}
      <AppLoadingState message="جارٍ التحميل..." />
    {:else if !accessDenied}
      {#if nodeType === 'WILAYA' && isAdmin}
        <!-- أمان الحسابات: كلمة مرور المسؤول العام -->
        <div class="mt-8">
          <AppCard>
            <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
              <div class="w-12 h-12 bg-amber-50 dark:bg-amber-900/20 rounded-lg flex items-center justify-center">
                <svg class="w-6 h-6 text-amber-600 dark:text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/>
                </svg>
              </div>
              <div>
                <h3 class="font-semibold text-gray-800 dark:text-gray-100">أمان الحسابات</h3>
                <p class="text-sm text-gray-500 dark:text-gray-400">كلمة مرور المسؤول العام</p>
              </div>
            </div>
            <AppAlert intent="info">
              <p class="text-sm leading-relaxed">
                كلمة مرور المسؤول العام (<b>Fleet Admin Password</b>) هي كلمة مرور تسجيل الدخول
                العادي لحساب <code class="font-mono">admin</code> — وهي <b>مختلفة تماماً</b> عن
                <b>كلمة مرور المفتاح الإداري</b> (تُستخدم في تبويب «المفتاح الإداري» للتعافي
                والتحقق عالي الضمان). تُصدَّر كلمة مرور المسؤول العام إلى الوحدات عبر حزمة الحسابات (B8).
              </p>
            </AppAlert>

            <form class="mt-4 space-y-3" on:submit|preventDefault={handleSetFleetPassword} novalidate>
              <AppInput
                id="fleet-password"
                label="كلمة مرور المسؤول العام"
                type="password"
                bind:value={fleetPassword}
                placeholder="8 أحرف على الأقل"
                autocomplete="new-password"
                required
                disabled={$operationLoading}
              />
              <AppInput
                id="fleet-confirm-password"
                label="تأكيد كلمة مرور المسؤول العام"
                type="password"
                bind:value={fleetConfirmPassword}
                placeholder="أعد إدخال كلمة المرور"
                autocomplete="new-password"
                required
                disabled={$operationLoading}
              />
              {#if fleetError}
                <AppAlert intent="danger" dismissible on:dismiss={() => fleetError = ''}>{fleetError}</AppAlert>
              {/if}
              {#if fleetSuccess}
                <AppAlert intent="success" dismissible on:dismiss={() => fleetSuccess = ''}>{fleetSuccess}</AppAlert>
              {/if}
              <AppButton
                type="submit"
                variant="primary"
                fullWidth
                loading={$operationLoading}
              >
                تعيين كلمة مرور المسؤول العام
              </AppButton>
            </form>
          </AppCard>
        </div>

        <!-- مزامنة حساب المدير العام (admin_access): تصدير أسطولي -->
        <div class="mt-8">
          <AppCard>
            <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
              <div class="w-12 h-12 bg-blue-50 dark:bg-blue-900/20 rounded-lg flex items-center justify-center">
                <svg class="w-6 h-6 text-civil-blue dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                </svg>
              </div>
              <div>
                <h3 class="font-semibold text-gray-800 dark:text-gray-100">مزامنة حساب المدير العام (admin)</h3>
                <p class="text-sm text-gray-500 dark:text-gray-400">تصدير حزمة admin_access لجميع الوحدات المُسجَّلة</p>
              </div>
            </div>
            <AppAlert intent="info">
              <p class="text-sm leading-relaxed">
                تُصدَّر الحزمة <strong>لجميع الوحدات المُسجَّلة</strong> دفعة واحدة
                (SEC-033): الخلفية تحدد الوحدات المستهدفة من السجل المحلي، وكل
                وحدة لها تسلسل نقل مستقل وحزمة موقّعة خاصة بها. تحمل كل حزمة
                حساب <code class="font-mono">admin</code> فقط، ولا تُعدّل حساب
                مشغّل الوحدة: اسم المستخدم وكلمة المرور المُوفرَّان عبر ملف
                <code class="font-mono">.unit</code> يبقيان كما هما.
              </p>
            </AppAlert>
            {#if b8ExportProgress}
              <div class="mt-2">
                <AppAlert intent="info">{b8ExportProgress}</AppAlert>
              </div>
            {/if}
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-3 leading-relaxed">
              الحزمة موقّعة ومشفّرة بالكامل من الخلفية ولا تعرض محتوياتها هنا.
              إذا لم تُضبط كلمة مرور المسؤول العام بعد، يرفض الخادم التصدير (Fail-Closed).
            </p>
            <AppButton
              variant="secondary"
              fullWidth
              class="mt-3"
              disabled={!!b8ExportProgress || $operationLoading}
              loading={!!b8ExportProgress}
              on:click={handleExportAdminAccess}
            >
              تصدير حزمة حساب المدير العام الموقّعة والمشفّرة
            </AppButton>
          </AppCard>
        </div>
      {:else if nodeType === 'UNIT'}
        <!-- The confirmation survives the projection flip: `refreshCurrentUser`
             clears `must_change_password`, which unmounts the forced card, so
             the result is announced here rather than inside it. -->
        {#if selfChangeSuccess}
          <AppAlert intent="success" dismissible on:dismiss={() => selfChangeSuccess = ''}>{selfChangeSuccess}</AppAlert>
        {/if}
        {#if mustChangePassword}
          <!-- ADR-0063 §5/§6 — forced credential state: the ONLY actionable
               surface while `must_change_password` is active. Every other
               section stays hidden because the backend rejects all
               non-allowlisted commands until the change succeeds. -->
          <div class="mt-8">
            <AppCard>
              <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
                <div class="w-12 h-12 bg-amber-50 dark:bg-amber-900/20 rounded-lg flex items-center justify-center">
                  <svg class="w-6 h-6 text-amber-600 dark:text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/>
                  </svg>
                </div>
                <div>
                  <h3 class="font-semibold text-gray-800 dark:text-gray-100">تغيير كلمة المرور الإلزامي</h3>
                  <p class="text-sm text-gray-500 dark:text-gray-400">يجب استبدال كلمة المرور المؤقتة قبل متابعة العمل</p>
                </div>
              </div>
              <AppAlert intent="warning">
                <p class="text-sm leading-relaxed">
                  هذه الحساب مُفعَّل عليه وضع «تغيير كلمة المرور الإلزامي»:
                  يرفض الخلفية كل العمليات الأخرى حتى تُستبدل كلمة المرور المؤقتة
                  بكلمة مرور جديدة. لا يمكن إرجاع هذا الوضع من الواجهة.
                </p>
              </AppAlert>
              <form class="mt-4 space-y-3" on:submit|preventDefault={handleChangeOwnPassword} novalidate>
                <AppInput
                  id="self-change-current"
                  label="كلمة المرور الحالية"
                  type="password"
                  bind:value={selfChangeCurrent}
                  autocomplete="current-password"
                  required
                  disabled={$selfChangeLoading}
                />
                <AppInput
                  id="self-change-new"
                  label="كلمة المرور الجديدة"
                  type="password"
                  bind:value={selfChangeNew}
                  placeholder="8 أحرف على الأقل، مع حرف كبير وحرف صغير ورقم"
                  autocomplete="new-password"
                  required
                  disabled={$selfChangeLoading}
                />
                <AppInput
                  id="self-change-confirm"
                  label="تأكيد كلمة المرور الجديدة"
                  type="password"
                  bind:value={selfChangeConfirm}
                  placeholder="أعد إدخال كلمة المرور"
                  autocomplete="new-password"
                  required
                  disabled={$selfChangeLoading}
                />
                {#if selfChangeError}
                  <AppAlert intent="danger" dismissible on:dismiss={() => selfChangeError = ''}>{selfChangeError}</AppAlert>
                {/if}
                <AppButton
                  type="submit"
                  variant="primary"
                  fullWidth
                  loading={$selfChangeLoading}
                >
                  تعيين كلمة المرور الجديدة
                </AppButton>
              </form>
            </AppCard>
          </div>
        {:else}
        {#if isAdmin}
          <!-- ADR-0063 §7.1/D6 — local admin reset of the canonical operator.
               Admin-only (the backend refuses every other actor); visible only
               outside the forced state because a forced operator cannot reach
               any admin surface anyway. No target selector: the backend derives
               the operator row from the local UNIT code. -->
          <div class="mt-8">
            <AppCard>
              <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
                <div class="w-12 h-12 bg-rose-50 dark:bg-rose-900/20 rounded-lg flex items-center justify-center">
                  <svg class="w-6 h-6 text-rose-600 dark:text-rose-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/>
                  </svg>
                </div>
                <div>
                  <h3 class="font-semibold text-gray-800 dark:text-gray-100">إعادة تعيين كلمة مرور مشغّل الوحدة</h3>
                  <p class="text-sm text-gray-500 dark:text-gray-400">تغيير كلمة مرور الحساب المحلي <code class="font-mono">user</code> (ADR-0063 §7.1)</p>
                </div>
              </div>
              <AppAlert intent="warning">
                <p class="text-sm leading-relaxed">
                  تُعيّن كلمة مرور مؤقتة لحساب مشغّل الوحدة المحلي
                  (<code class="font-mono">user</code>). عند تسجيل الدخول التالي
                  يُجمَع على المشغّل استبدالها بموجب وضع «تغيير كلمة المرور
                  الإلزامي» — لا يمكن إرجاعه من الواجهة. لا تقبل الخلفية كلمة
                  المرور <code class="font-mono">0000</code>، ويُستبدَل الهدف
                  من رمز الوحدة المحلي حصراً (لا اختيار اسم مستخدم أو وحدة هنا).
                </p>
              </AppAlert>
              <form class="mt-4 space-y-3" on:submit|preventDefault={handleResetUnitUserPassword} novalidate>
                <AppInput
                  id="unit-reset-temp"
                  label="كلمة مرور مؤقتة"
                  type="password"
                  bind:value={resetUnitPassword}
                  placeholder="8 أحرف على الأقل، مع حرف كبير وحرف صغير ورقم"
                  autocomplete="new-password"
                  required
                  disabled={$unitResetLoading}
                />
                <AppInput
                  id="unit-reset-confirm"
                  label="تأكيد كلمة المرور المؤقتة"
                  type="password"
                  bind:value={resetUnitConfirm}
                  placeholder="أعد إدخال كلمة المرور"
                  autocomplete="new-password"
                  required
                  disabled={$unitResetLoading}
                />
                {#if resetUnitError}
                  <AppAlert intent="danger" dismissible on:dismiss={() => resetUnitError = ''}>{resetUnitError}</AppAlert>
                {/if}
                {#if resetUnitSuccess}
                  <AppAlert intent="success" dismissible on:dismiss={() => resetUnitSuccess = ''}>{resetUnitSuccess}</AppAlert>
                {/if}
                <AppButton
                  type="submit"
                  variant="primary"
                  fullWidth
                  loading={$unitResetLoading}
                >
                  إعادة تعيين كلمة مرور مشغّل الوحدة
                </AppButton>
              </form>
            </AppCard>
          </div>
        {/if}
        <!-- مزامنة حساب المدير العام (admin_access): استيراد أسطولي -->
        <div class="mt-8">
          <AppCard>
            <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
              <div class="w-12 h-12 bg-blue-50 dark:bg-blue-900/20 rounded-lg flex items-center justify-center">
                <svg class="w-6 h-6 text-civil-blue dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                </svg>
              </div>
              <div>
                <h3 class="font-semibold text-gray-800 dark:text-gray-100">مزامنة حساب المدير العام (admin)</h3>
                <p class="text-sm text-gray-500 dark:text-gray-400">استيراد حساب المسؤول العام من عقدة WILAYA</p>
              </div>
            </div>
            <AppAlert intent="info">
              <p class="text-sm leading-relaxed">
                تزامن هذه الحزمة حساب <code class="font-mono">admin</code> فقط
                (كلمة المرور الأسطولية المعينة على عقدة WILAYA).
                <strong>لا تُعدّل حساب مشغّل الوحدة المحلي إطلاقًا:</strong> استمر
                باستخدام اسم المستخدم وكلمة المرور المُوفرَّين عبر ملف
                <code class="font-mono">.unit</code>. لا تحمل الحزمة أي مادة هوية
                أو مفاتيخ خاصة، ولا يُنشأ أي جلسة أو شهادة محلية بغير ذلك.
              </p>
            </AppAlert>
            {#if isAdmin}
              <p class="text-xs text-gray-500 dark:text-gray-500 mt-3 leading-relaxed">
                بحساب المسؤول المحلي يُعاد الاستيراد عبر مسار التفويض الإداري المعتاد؛ يقبل
                الخادم الطلب أو يرفضه وفق ضوابطه.
              </p>
            {/if}
            {#if b8ImportSuccess}
              <div class="mt-4">
                <AppAlert intent="success" dismissible on:dismiss={() => b8ImportSuccess = ''}>{b8ImportSuccess}</AppAlert>
              </div>
            {/if}
            {#if b8ImportError}
              <div class="mt-4">
                <AppAlert intent="danger" dismissible on:dismiss={() => b8ImportError = ''}>{b8ImportError}</AppAlert>
              </div>
            {/if}
            <AppButton
              variant="secondary"
              size="lg"
              fullWidth
              class="mt-3"
              loading={$b8ImportLoading}
              on:click={handleImportAdminAccess}
            >
              استيراد حزمة حساب المدير العام
            </AppButton>
          </AppCard>
        </div>
        {/if}
      {/if}
    {/if}
  </div>
</Layout>