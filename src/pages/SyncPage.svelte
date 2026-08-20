<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    importDailyReportPackage,
    importMonthlySummaryPackage,
    importStockMovementsPackage,
    setFleetAdminPassword,
    exportIdentityAccessPackage,
  } from '../lib/contracts';
  import { openFile, saveFile } from '../lib/tauri';
  import { listUnits, getSettings } from '../lib/contracts';
  import type { Unit, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';
  import { formatErrorMessage } from '../lib/errors';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const initialOp = createOperation({ scope });
  const loading = initialOp.loading;
  const initialError = initialOp.error;

  const { loading: operationLoading, guard } = createOperationGuard({ scope });

  // @category ProjectionState
  let units: Unit[] = [];
  // @category ProjectionState
  let settings: Settings | null = null;
  // @category TransientState
  let error = '';
  // @category TransientState
  let success = '';
  // @category TransientState
  let importProgress = '';
  // @category UiState
  let selectedUnit: string = '';

  // B8 (ADR-0040): fleet admin credential — the WILAYA Admin-only normal
  // account credential, propagated to UNITs through the identity_access
  // package. It is a DIFFERENT secret from the Admin Key passphrase; it is
  // never logged, never persisted in frontend storage, and cleared from the
  // fields after submission.
  // @category TransientState
  let fleetPassword = '';
  // @category TransientState
  let fleetConfirmPassword = '';
  // @category TransientState
  let fleetError = '';
  // @category TransientState
  let fleetSuccess = '';
  // @category TransientState
  let b8ExportProgress = '';

  // @category ProjectionState
  $: selectedUnitCode = units.find((u) => u.id === selectedUnit)?.code ?? '';

  onMount(async () => {
    await initialOp.run(async () => {
      settings = await getSettings();
      if (settings?.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
        if (units.length > 0) {
          selectedUnit = units[0].id;
        }
      }
    });
  });

  async function importDailyReportPackageSync() {
    if (!selectedUnit) {
      error = 'الرجاء اختيار وحدة';
      return;
    }

    await guard(async () => {
      try {
        const selected = await openFile({
          multiple: false,
          filters: [{
            name: 'حزمة المزامنة',
            extensions: ['sync']
          }]
        });

        if (selected) {
          importProgress = 'استيراد تقرير يومي مؤمن...';
          error = '';
          success = '';
          const result = await importDailyReportPackage(selected as string, selectedUnit);
          success = `تم استيراد ${result.report_count} تقرير (${result.item_count} عناصر استهلاك)`;
          importProgress = '';
        }
      } catch (e) {
        error = formatErrorMessage(e);
        importProgress = '';
      }
    });
  }


  async function importMonthlyReportSync() {
    if (!selectedUnit) {
      error = 'يرجى اختيار وحدة';
      return;
    }

    await guard(async () => {
      try {
        const selected = await openFile({
          multiple: false,
          filters: [{
            name: 'حزمة المزامنة',
            extensions: ['sync']
          }]
        });

        if (selected) {
          importProgress = 'استيراد حزمة شهرية مؤمنة...';
          error = '';
          success = '';
          const result = await importMonthlySummaryPackage(selected as string, selectedUnit);
          success = `تم استيراد الملخص الشهري (${result.report_count} تقارير مسجلة في الحزمة)`;
          importProgress = '';
        }
      } catch (e) {
        error = formatErrorMessage(e);
        importProgress = '';
      }
    });
  }



  async function importStockMovementsPackageSync() {
    if (!selectedUnit) {
      error = 'يرجى اختيار وحدة أولاً';
      return;
    }

    await guard(async () => {
      try {
        const selected = await openFile({
          multiple: false,
          filters: [{
            name: 'حزمة المزامنة',
            extensions: ['sync']
          }]
        });

        if (selected) {
          importProgress = 'استيراد حزمة حركات مؤمنة...';
          error = '';
          success = '';
          const result = await importStockMovementsPackage(selected as string, selectedUnit);
          success = `تم استيراد ${result.movement_count} حركة مخزون بنجاح (المعرف الفريد للحزمة: ${result.file_hash.substring(0, 8)}...)`;
          importProgress = '';
        }
      } catch (e) {
        error = formatErrorMessage(e);
        importProgress = '';
      }
    });
  }

  // B8 (ADR-0040): WILAYA Admin initializes the fleet admin password. The
  // backend command is the sole authority (ManageAccountSync → Wilaya +
  // AdminOnly); this form only relays the new password — validation rules are
  // relayed for UX, the backend enforces its own. Passwords are cleared after
  // submission and never rendered back.
  async function handleSetFleetPassword() {
    if (fleetPassword.length < 8) {
      fleetError = 'كلمة المرور قصيرة جداً — يجب أن تكون 8 أحرف على الأقل';
      return;
    }
    if (fleetPassword !== fleetConfirmPassword) {
      fleetError = 'كلمتا المرور غير متطابقتين';
      return;
    }
    await guard(async () => {
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

  // B8 (ADR-0040): WILAYA-only signed/encrypted identity_access export. The
  // backend builds, signs, and encrypts the package (fail-closed while the
  // fleet password is unset); the UI only selects the target unit and the
  // destination file — package contents never enter the frontend.
  async function handleExportIdentityAccess() {
    const code = selectedUnitCode;
    if (!code) {
      error = 'الرجاء اختيار وحدة أولاً';
      return;
    }
    await guard(async () => {
      try {
        const selected = await saveFile({
          defaultPath: `grpc-identity-access-${code}.sync`,
          filters: [{ name: 'حزمة الحسابات (B8)', extensions: ['sync'] }],
        });
        if (!selected) return;
        b8ExportProgress = `تصدير حزمة الحسابات (B8) للوحدة ${code}...`;
        error = '';
        success = '';
        const result = await exportIdentityAccessPackage(code, selected as string);
        success =
          `تم تصدير حزمة الحسابات (B8) للوحدة ${code} (${result.record_count} سجلات) — تحمل بيانات اعتماد المسؤول العام وحساب الوحدة.`;
        b8ExportProgress = '';
      } catch (e) {
        error = formatErrorMessage(e);
        b8ExportProgress = '';
      }
    });
  }
</script>

<Layout nodeType="WILAYA" title="المزامنة" subtitle="استيراد تقارير الوحدات">
  <div dir="rtl">
    <AppPageHeader title="المزامنة" subtitle="استيراد تقارير الوحدات" />

    {#if error || $initialError}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => { error = ''; initialOp.error.set(null); }}>{error || $initialError}</AppAlert>
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => success = ''}>{success}</AppAlert>
      </div>
    {/if}

    {#if importProgress}
      <div class="mb-4">
        <AppAlert intent="info">
          <div class="flex items-center">
            <svg class="animate-spin -ml-1 mr-3 h-5 w-5" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            {importProgress}
          </div>
        </AppAlert>
      </div>
    {/if}

    {#if $loading}
      <AppLoadingState message="جارٍ التحميل..." />
    {:else if units.length === 0}
      <AppCard padding="none">
        <AppEmptyState
          title="لا يوجد وحدات متاحة"
          description="أنشئ وحدات أولاً لتتمكن من المزامنة."
          icon="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
        >
          <svelte:fragment slot="action">
            <a href="#/wilaya/units">
              <AppButton variant="primary">أنشئ وحدة أولاً</AppButton>
            </a>
          </svelte:fragment>
        </AppEmptyState>
      </AppCard>
    {:else}
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
        <!-- Unit Selection -->
        <div class="md:col-span-2">
          <AppCard>
            <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">اختيار الوحدة</h2>
            <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">اختر الوحدة التي تستورد تقريرها</p>
            <AppSelect
              id="unit-select"
              label=""
              bind:value={selectedUnit}
            >
              {#each units as unit}
                <option value={unit.id}>{unit.code} - {unit.name}</option>
              {/each}
            </AppSelect>
          </AppCard>
        </div>

        <!-- Import Daily Report -->
        <AppCard>
          <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
            <div class="w-12 h-12 bg-blue-50 dark:bg-blue-900/20 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-civil-blue dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800 dark:text-gray-100">تقرير يومي</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">حزمة المزامنة (.sync) هي المسار الحالي للمزامنة</p>
            </div>
          </div>
          <div class="space-y-2">
            <AppButton
              variant="primary"
              fullWidth
              disabled={!selectedUnit || !!importProgress || $operationLoading}
              loading={importProgress === 'استيراد تقرير يومي مؤمن...'}
              on:click={importDailyReportPackageSync}
              ariaLabel="هذا هو مسار المزامنة الرسمي بين العقد"
            >
              استيراد حزمة المزامنة (.sync)
            </AppButton>
          </div>
        </AppCard>

        <!-- Import Monthly Report -->
        <AppCard>
          <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
            <div class="w-12 h-12 bg-green-50 dark:bg-green-900/20 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800 dark:text-gray-100">تقرير شهر</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">حزمة المزامنة (.sync) هي المسار الحالي للمزامنة</p>
            </div>
          </div>
          <div class="space-y-2">
            <AppButton
              variant="primary"
              fullWidth
              disabled={!selectedUnit || !!importProgress || $operationLoading}
              loading={importProgress === 'استيراد حزمة شهرية مؤمنة...'}
              on:click={importMonthlyReportSync}
              ariaLabel="هذا هو مسار المزامنة الرسمي بين العقد"
            >
              استيراد حزمة المزامنة (.sync)
            </AppButton>
          </div>
        </AppCard>

        <!-- Import Stock Movements -->
        <AppCard>
          <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
            <div class="w-12 h-12 bg-purple-50 dark:bg-purple-900/20 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-purple-600 dark:text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800 dark:text-gray-100">حركات المخزون</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">استورد حركات المخزون من الوحدة</p>
            </div>
          </div>
          <div class="space-y-2">
            <AppButton
              variant="primary"
              fullWidth
              class="bg-purple-600 hover:bg-purple-700 dark:bg-purple-600 dark:hover:bg-purple-700 border-purple-700"
              disabled={!selectedUnit || !!importProgress || $operationLoading}
              loading={importProgress === 'استيراد حزمة حركات مؤمنة...'}
              on:click={importStockMovementsPackageSync}
              ariaLabel="استيراد الحزمة المشفرة والموقعة رقمياً للمزامنة الآمنة"
            >
              استيراد حزمة حركات (.sync)
            </AppButton>
          </div>
        </AppCard>
      </div>

      <div class="mt-8">
        <AppCard>
          <div class="flex items-center gap-3 mb-4 border-b border-gray-100 dark:border-gray-700 pb-4">
            <div class="w-12 h-12 bg-amber-50 dark:bg-amber-900/20 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-amber-600 dark:text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800 dark:text-gray-100">حسابات العقد (B8)</h3>
              <p class="text-sm text-gray-500 dark:text-gray-400">كلمة مرور المسؤول العام + حزمة الحسابات (identity_access)</p>
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

          <div class="mt-6 pt-4 border-t border-gray-100 dark:border-gray-700">
            <h4 class="text-sm font-semibold text-gray-700 dark:text-gray-100 mb-3">
              تصدير حزمة الحسابات (B8) لوحدة
            </h4>
            <AppSelect id="b8-export-unit-select" label="" bind:value={selectedUnit}>
              {#each units as unit}
                <option value={unit.id}>{unit.code} - {unit.name}</option>
              {/each}
            </AppSelect>
            {#if b8ExportProgress}
              <div class="mt-2">
                <AppAlert intent="info">{b8ExportProgress}</AppAlert>
              </div>
            {/if}
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-3 leading-relaxed">
              الحزمة موقّعة ومشفّرة بالكامل من الخلفية وتحمل بيانات اعتماد المسؤول العام وحساب
              الوحدة — لا تُعرض محتوياتها هنا. إذا لم تُضبط كلمة مرور المسؤول العام بعد، يرفض
              الخادم التصدير (Fail-Closed).
            </p>
            <AppButton
              variant="secondary"
              fullWidth
              class="mt-3"
              disabled={!selectedUnit || !!b8ExportProgress || $operationLoading}
              loading={!!b8ExportProgress}
              on:click={handleExportIdentityAccess}
            >
              تصدير حزمة الحسابات (B8) الموقّعة والمشفّرة
            </AppButton>
          </div>
        </AppCard>
      </div>

      <div class="mt-8">
        <AppAlert intent="info" title="كيفية استيراد تقارير الوحدة">
          <ol class="text-sm list-decimal list-inside space-y-1 mt-2">
            <li><strong>من عقدة الوحدة:</strong> صدّر الملفات المطلوبة:
              <ul class="mr-4 mt-1 text-xs">
                <li>• تقرير يومي: صفحة "التقارير" → تصدير حزمة .sync</li>
                <li>• تقرير شهري: صفحة "التقارير" → تصدير حزمة .sync</li>
                <li>• <strong>حركات المخزون: صفحة "المخزون" → تصدير حزمة حركات (.sync)</strong></li>
              </ul>
            </li>
            <li><strong>من عقدة الولاية:</strong> اختر الوحدة المناسبة ثم استورد كل ملف</li>
          </ol>
        </AppAlert>
      </div>
    {/if}
  </div>
</Layout>
