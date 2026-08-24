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
    listUnits,
  } from '../lib/contracts';
  import { openFile, saveFile } from '../lib/tauri';
  import type { Settings, Unit } from '../lib/types';
  import { push } from 'svelte-spa-router';
  import { currentUser as userStore } from '../lib/session';
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

  // @category SessionState
  $: user = $userStore;
  // @category UiState
  $: isAdmin = user?.role === 'Admin';

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
  // ADR-0053: each package targets ONE authoritative UNIT; the backend
  // validates `unitCode` against `units.code` and keys the per-target
  // transport stream with it.
  // @category TransientState
  let b8ExportProgress = '';
  // @category ProjectionState — authoritative UNIT targets (ADR-0053)
  let units: Unit[] = [];
  // @category UiState — selected transport target for the admin_access package
  let selectedUnitCode = '';

  // ── UNIT: B8 import (مزامنة الحسابات) ─────────────────────────────────────
  // @category TransientState
  let b8ImportSuccess = '';
  // @category TransientState
  let b8ImportError = '';

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
      // ADR-0053: load authoritative UNIT targets for per-target exports.
      if (nodeType === 'WILAYA' && settings?.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
        if (!units.some((u) => u.code === selectedUnitCode)) {
          selectedUnitCode = units[0]?.code ?? '';
        }
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

  // WILAYA-only signed/encrypted admin_access export (ADR-0051 + ADR-0053).
  // The backend builds, signs, and encrypts the package for ONE authoritative
  // UNIT target (fail-closed while the fleet password is unset); the selected
  // `unitCode` is validated server-side against `units.code`. Package
  // contents never enter the frontend.
  async function handleExportAdminAccess() {
    await accountOp.guard(async () => {
      try {
        if (!selectedUnitCode) {
          fleetError = 'يجب اختيار الوحدة الهدف — لا توجد وحدات مُسجَّلة.';
          return;
        }
        const selected = await saveFile({
          defaultPath: `grpc-admin-access.sync`,
          filters: [{ name: 'حزمة حساب المدير العام', extensions: ['sync'] }],
        });
        if (!selected) return;
        b8ExportProgress = 'تصدير حزمة حساب المدير العام (admin)...';
        fleetError = '';
        fleetSuccess = '';
        const result = await exportAdminAccessPackage(selected as string, selectedUnitCode);
        const unitLabel = units.find((u) => u.code === selectedUnitCode)?.name ?? selectedUnitCode;
        fleetSuccess =
          `تم تصدير حزمة حساب المدير العام (${result.record_count} سجلات) للوحدة ${unitLabel} — لا تُعدّل حساب مشغّل الوحدة إطلاقًا.`;
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
                <p class="text-sm text-gray-500 dark:text-gray-400">تصدير حزمة admin_access لوحدة مستهدفة</p>
              </div>
            </div>
            <AppAlert intent="info">
              <p class="text-sm leading-relaxed">
                تُصدَّر الحزمة <strong>لوحدة واحدة مستهدفة</strong> في كل مرة
                (ADR-0053): كل وحدة لها تسلسل نقل مستقل، ويُتحقّق من رمز الوحدة
                لدى الخادم. تحمل الحزمة حساب <code class="font-mono">admin</code>
                فقط، ولا تُعدّل حساب مشغّل الوحدة: اسم المستخدم وكلمة المرور
                المُوفرَّان عبر ملف <code class="font-mono">.unit</code> يبقيان كما هما.
              </p>
            </AppAlert>
            <div class="mt-3">
              <label for="admin-access-target-unit" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                الوحدة الهدف
              </label>
              <select
                id="admin-access-target-unit"
                bind:value={selectedUnitCode}
                disabled={!!b8ExportProgress || $operationLoading || units.length === 0}
                class="px-3 py-1.5 border border-gray-200 dark:border-gray-700 rounded-lg text-sm text-gray-800 dark:text-white bg-white dark:bg-gray-700"
              >
                {#if units.length === 0}
                  <option value="">لا توجد وحدات مُسجَّلة</option>
                {:else}
                  {#each units as unit (unit.id)}
                    <option value={unit.code}>{unit.code} — {unit.name}</option>
                  {/each}
                {/if}
              </select>
            </div>
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
  </div>
</Layout>