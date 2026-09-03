<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { closeFiscalYear, getFiscalYearStatus, exportFiscalClosurePackage, previewFiscalClosurePackage, applyFiscalClosurePackage, getFiscalTransitionHistory, listFiscalPackageRegistry, updateFiscalPackageRetentionStatus } from '../lib/contracts';
  import { saveFile, openFile, showAsk } from '../lib/tauri';
  import { listProducts, getSettings, listFiscalTaxPolicies, setFiscalTaxPolicy } from '../lib/contracts';
  import type { 
    FiscalYearStatus, 
    FiscalClosurePreview, 
    FiscalClosureApplyResult,
    FiscalTransitionHistoryEntry,
    FiscalPackageRegistryEntry,
    FiscalYearTaxPolicy
  } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperationGuard } from '../lib/operationGuard';
  import { formatErrorMessage } from '../lib/errors';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppSection from '../lib/components/ui/AppSection.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const { loading: opLoading, guard } = createOperationGuard({ scope });

  // @category ProjectionState
  let nodeType: 'WILAYA' | 'UNIT' | null = null;
  // @category UiState
  let currentYear = new Date().getFullYear();
  // @category UiState
  let nextYear = currentYear + 1;
  // @category TransientState
  let confirmation = '';
  // @category TransientState
  let message = '';
  // @category ProjectionState
  let productCount = 0;
  // @category ProjectionState
  let fiscalStatus: FiscalYearStatus | null = null;
  // @category ProjectionState
  let taxPolicies: FiscalYearTaxPolicy[] = [];
  // @category TransientState
  let taxRateInput = '';

  // Import/Preview state
  // @category ProjectionState
  let preview: FiscalClosurePreview | null = null;
  // @category TransientState
  let selectedFilePath = '';
  // @category TransientState
  let applyConfirmation = '';
  
  // History & Registry state
  // @category ProjectionState
  let transitionHistory: FiscalTransitionHistoryEntry[] = [];
  // @category ProjectionState
  let packageRegistry: FiscalPackageRegistryEntry[] = [];
  // @category TransientState
  let retentionConfirmation = '';
  // @category UiState
  let selectedPackageForRetention: FiscalPackageRegistryEntry | null = null;
  // @category UiState
  let targetRetentionStatus: 'ARCHIVED' | 'RETIRED' | null = null;

  async function load() {
    const settings = await getSettings();
    nodeType = settings.node_type as 'WILAYA' | 'UNIT';
    
    fiscalStatus = await getFiscalYearStatus(settings.current_year);
    if (fiscalStatus) {
      currentYear = fiscalStatus.year;
      nextYear = currentYear + 1;
    } else {
      currentYear = settings.current_year;
      nextYear = currentYear + 1;
    }

    const products = await listProducts();
    productCount = products.length;

    if (nodeType === 'WILAYA') {
      taxPolicies = await listFiscalTaxPolicies();
    }

    await refreshHistory();
  }

  async function saveTaxPolicy() {
    const rate = parseFloat(taxRateInput);
    if (Number.isNaN(rate) || rate < 0 || rate > 100) {
      message = 'يرجى إدخال نسبة ضريبية صحيحة (0 - 100).';
      return;
    }

    await guard(async () => {
      message = '';
      try {
        await setFiscalTaxPolicy({ fiscal_year: currentYear, tva_rate: rate });
        message = `تم تحديث سياسة الضريبة (TVA) للسنة ${currentYear} إلى ${rate}%.`;
        taxRateInput = '';
        taxPolicies = await listFiscalTaxPolicies();
      } catch (error) {
        message = formatErrorMessage(error);
      }
    });
  }

  async function refreshHistory() {
    try {
      transitionHistory = await getFiscalTransitionHistory();
      packageRegistry = await listFiscalPackageRegistry();
    } catch (e) {
      console.error('Failed to load history:', e);
    }
  }

  async function handleUpdateRetention(transitionId: string, status: 'ARCHIVED' | 'RETIRED') {
    const expected = `${status}-PACKAGE`;
    if (retentionConfirmation !== expected) {
      message = `يرجى كتابة رمز التأكيد ${expected} بشكل صحيح.`;
      return;
    }

    await guard(async () => {
      try {
        await updateFiscalPackageRetentionStatus(transitionId, status, retentionConfirmation);
        message = `تم تحديث حالة الحزمة إلى ${status} بنجاح.`;
        selectedPackageForRetention = null;
        retentionConfirmation = '';
        await refreshHistory();
      } catch (error) {
        message = formatErrorMessage(error);
      }
    });
  }

  async function executeClose() {
    if (confirmation !== String(currentYear)) {
      message = 'رقم السنة غير صحيح. تم رفض العملية.';
      return;
    }

    const confirmed = await showAsk(`سيتم إغلاق السنة المالية ${currentYear} وفتح ${nextYear}. لا يمكن التراجع عن العملية.`, {
      title: 'تحذير: إغلاق السنة المالية',
      kind: 'warning',
      okLabel: 'نعم',
      cancelLabel: 'لا',
    });
    if (!confirmed) return;

    await guard(async () => {
      message = '';

      try {
        const result = await closeFiscalYear({
          year: currentYear,
          next_year: nextYear,
        });

        message = `تم إغلاق ${result.closed_year} وفتح ${result.opened_year} بنجاح.`;
        await load(); // Reload state
        confirmation = '';
        await refreshHistory();
      } catch (error) {
        message = formatErrorMessage(error);
      }
    });
  }

  async function handleExportPackage() {
    if (!fiscalStatus || fiscalStatus.status === 'open') {
      message = 'يجب إغلاق السنة المالية أولاً قبل تصدير حزمة الترخيص.';
      return;
    }
    await handleExportRegistryPackage(fiscalStatus.year, fiscalStatus.year + 1, fiscalStatus.closed_at || new Date().toISOString());
  }

  async function handleExportRegistryPackage(closedYear: number, openedYear: number, timestamp: string, transitionId: string | null = null) {
    const filePath = await saveFile({
      title: 'حفظ حزمة ترخيص إغلاق السنة المالية',
      defaultPath: `fiscal_closure_${closedYear}.fiscal-close.sync`,
      filters: [{ name: 'Fiscal Closure Package', extensions: ['sync'] }]
    });

    if (!filePath) return;

    await guard(async () => {
      try {
        await exportFiscalClosurePackage(
          closedYear,
          openedYear,
          timestamp,
          filePath,
          transitionId
        );
        message = `تم تصدير حزمة الترخيص للسنة ${closedYear} بنجاح إلى: ${filePath}`;
        await refreshHistory();
      } catch (error) {
        message = formatErrorMessage(error);
      }
    });
  }

  async function handleSelectPackage() {
    const filePath = await openFile({
      title: 'اختر حزمة ترخيص إغلاق السنة المالية',
      multiple: false,
      filters: [{ name: 'Fiscal Closure Package', extensions: ['sync'] }]
    });

    if (!filePath || Array.isArray(filePath)) return;

    selectedFilePath = filePath;
    await guard(async () => {
      preview = null;
      message = '';

      try {
        preview = await previewFiscalClosurePackage(selectedFilePath);
        if (!preview.validation_ok) {
          message = 'فشل التحقق المسبق من الحزمة. راجع التفاصيل أدناه.';
        }
      } catch (error) {
        message = formatErrorMessage(error);
        selectedFilePath = '';
      }
    });
  }

  async function handleApplyPackage() {
    if (!preview || !preview.validation_ok) return;
    if (applyConfirmation !== 'APPLY-FISCAL-TRANSITION') {
      message = 'يرجى كتابة رمز التأكيد بشكل صحيح.';
      return;
    }

    await guard(async () => {
      message = '';

      try {
        const result = await applyFiscalClosurePackage(selectedFilePath, applyConfirmation);
        message = `تم تطبيق الانتقال المالي للسنة ${result.closed_year} بنجاح. تم ترحيل ${result.snapshot_count} سجل.`;
        preview = null;
        selectedFilePath = '';
        applyConfirmation = '';
        await load();
        await refreshHistory();
      } catch (error) {
        message = formatErrorMessage(error);
      }
    });
  }

  onMount(load);

  // Helper to determine badge state
  // @category UiState
  $: isExpired = preview && new Date() > new Date(preview.authorized_execution_window.expires_at);
  // @category UiState
  $: isNotYetStarted = preview && new Date() < new Date(preview.authorized_execution_window.not_before);
  // @category UiState
  $: isReplay = preview && preview.validation_issues.some(i => i.includes('replay'));
  // @category UiState
  $: isInvalidSignature = message.includes('فشل التحقق من توقيع حزمة الإغلاق المالي') || message.includes('رُفضت الحزمة');
</script>

<Layout {nodeType} title="إدارة السنة المالية">
  <div class="max-w-3xl space-y-6 mx-auto p-4 md:p-0" dir="rtl">
    
    <AppPageHeader title="إدارة السنة المالية" />

    <AppCard padding="md">
      <div class="flex justify-between items-center">
        <div>
          <p class="text-sm text-gray-500 dark:text-gray-400">السنة المالية المفتوحة حالياً</p>
          <p class="text-2xl font-bold text-gray-800 dark:text-white">{currentYear}</p>
        </div>
        <div class="text-right">
          <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">حالة النظام</p>
          <AppBadge intent={fiscalStatus?.status === 'open' ? 'success' : 'danger'}>
            {fiscalStatus?.status === 'open' ? 'مفتوح للعمليات' : 'مغلق مالياً'}
          </AppBadge>
        </div>
      </div>
    </AppCard>

    {#if nodeType === 'WILAYA'}
      <!-- WILAYA MODE: CLOSE & EXPORT -->
      {#if fiscalStatus?.status === 'open'}
        <AppAlert intent="danger" title="سلطة إغلاق السنة المالية (WILAYA)">
          <p>أنت تملك سلطة اتخاذ قرار الإغلاق المالي. سيؤدي هذا الإجراء إلى:</p>
          <ul class="list-disc pr-5 mt-2 space-y-1 text-sm">
            <li>قفل السنة المالية {currentYear} نهائياً</li>
            <li>توليد حزمة ترخيص للانتقال المالي للعقد التابعة</li>
            <li>فتح السنة المالية الجديدة {nextYear}</li>
          </ul>
          <p class="mt-3 font-semibold text-xs">
            ملاحظة (Operator Guide): الغلق المالي هو عملية لا رجعة فيها (Irreversible). التواريخ والبصمات الناتجة ستكون مسجلة بشكل دائم في سلسلة التدقيق المغلقة. التوكيد النصي مطلوب لمنع الغلق الخطأ.
          </p>
        </AppAlert>

        <AppCard padding="md">
          <h2 class="font-semibold border-b border-gray-200 dark:border-gray-700 pb-2 text-gray-800 dark:text-white mb-4">تنفيذ قرار الإغلاق</h2>
          <div class="grid grid-cols-2 gap-3 text-sm mb-4 bg-gray-50 dark:bg-gray-900 p-3 rounded-xl text-gray-700 dark:text-gray-300">
            <div>السنة الحالية: <span class="font-mono font-bold dark:text-white">{currentYear}</span></div>
            <div>السنة الجديدة: <span class="font-mono font-bold dark:text-white">{nextYear}</span></div>
          </div>

          <div class="mb-4">
            <AppInput
              id="year-confirmation"
              label="اكتب رقم السنة الحالية ({currentYear}) للتأكيد"
              bind:value={confirmation}
              placeholder={String(currentYear)}
            />
          </div>

          <AppButton
            variant="danger"
            fullWidth
            size="lg"
            disabled={$opLoading}
            loading={$opLoading}
            on:click={executeClose}
          >
            إصدار قرار إغلاق السنة {currentYear}
          </AppButton>
        </AppCard>
      {:else}
        <!-- ALREADY CLOSED IN WILAYA: ALLOW EXPORT -->
        <AppCard padding="lg" class="text-center">
          <div class="mx-auto w-16 h-16 bg-blue-100 dark:bg-blue-900/50 rounded-full flex items-center justify-center text-blue-600 dark:text-blue-400 mb-4">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04a11.367 11.367 0 01-1.091 5.496c.002.314.05.628.143.933a11.503 11.503 0 001.371 3.513c.176.326.362.641.551.944A12.026 12.026 0 0011.962 21.01a12.02 12.02 0 008.474-5.991c.401-.736.745-1.515 1.022-2.322a10.107 10.107 0 00.395-1.842 11.233 11.233 0 00-1.091-5.496z" />
            </svg>
          </div>
          <h2 class="text-xl font-bold text-blue-900 dark:text-blue-300 mb-2">تصدير ترخيص الانتقال</h2>
          <p class="text-blue-700 dark:text-blue-400 mb-6">السنة المالية {fiscalStatus?.year} مغلقة. يمكنك تصدير حزمة ترخيص الانتقال المالي (Remote Authorization Package).</p>
          
          <AppButton
            variant="primary"
            size="lg"
            disabled={$opLoading}
            loading={$opLoading}
            on:click={handleExportPackage}
          >
            تصدير حزمة الترخيص (.sync)
          </AppButton>
        </AppCard>
      {/if}

      <!-- TVA Policy (WILAYA) -->
      <AppCard padding="md">
        <h2 class="font-semibold border-b border-gray-200 dark:border-gray-700 pb-2 text-gray-800 dark:text-white mb-4">سياسة الضريبة (TVA)</h2>
        <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
          نسبة الضريبة على القيمة المضافة موحّدة لكل سنة مالية (سعر واحد لكل ولاية). تُجمّد عند إغلاق السنة المالية ولا يمكن تعديلها بعدها.
        </p>

        <div class="flex flex-wrap items-end gap-3">
          <div class="flex-1 min-w-[180px]">
            <AppInput
              id="tva-rate"
              label={`نسبة TVA للسنة ${currentYear} (%)`}
              type="number"
              min="0"
              max="100"
              placeholder="0"
              bind:value={taxRateInput}
            />
          </div>
          <AppButton
            variant="primary"
            loading={$opLoading}
            disabled={fiscalStatus?.status !== 'open' || $opLoading}
            on:click={saveTaxPolicy}
          >
            تطبيق السياسة
          </AppButton>
        </div>

        {#if taxPolicies.length > 0}
          <div class="mt-4">
            <AppTable
              empty={taxPolicies.length === 0}
              emptyMessage="لا توجد سياسة ضريبية محددة بعد"
            >
            <svelte:fragment slot="head">
              <th class="table-header">السنة المالية</th>
              <th class="table-header">النسبة (%)</th>
              <th class="table-header">الحالة</th>
              <th class="table-header">المُحدِّث</th>
            </svelte:fragment>
            {#each taxPolicies as policy}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
                <td class="table-cell font-bold text-gray-800 dark:text-white">{policy.fiscal_year}</td>
                <td class="table-cell tabular-nums">{policy.tva_rate.toFixed(2)}%</td>
                <td class="table-cell">
                  <AppBadge intent={policy.frozen ? 'warning' : 'success'} size="sm">
                    {policy.frozen ? 'مجمّدة' : 'قابلة للتعديل'}
                  </AppBadge>
                </td>
                <td class="table-cell">{policy.set_by}</td>
              </tr>
            {/each}
            </AppTable>
          </div>
        {/if}
      </AppCard>
    {:else}
      <AppCard padding="lg">
        <div class="flex items-start gap-4 mb-6">
          <div class="bg-indigo-100 dark:bg-indigo-900/40 p-3 rounded-xl text-indigo-600 dark:text-indigo-400 shrink-0">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
            </svg>
          </div>
          <div class="flex-1">
            <h2 class="text-lg font-bold text-gray-800 dark:text-white">تطبيق انتقال مالي مُصرّح من الولاية</h2>
            <p class="text-sm text-gray-600 dark:text-gray-400 mt-1">أنت تقوم بتنفيذ قرار إغلاق اتخذته الولاية مسبقاً. لا تملك الوحدات (UNIT) سلطة إغلاق مستقلة.</p>
          </div>
        </div>
        <AppAlert intent="info">
          <strong>ملاحظة أمان:</strong> الحزم محمية ضد إعادة التشغيل (Replay Protection). إذا تم تطبيق حزمة مسبقاً، سيتم رفضها منعاً لتكرار ترحيل الأرصدة.
        </AppAlert>

        {#if !preview}
          <div class="mt-6">
            <button
              on:click={handleSelectPackage}
              disabled={$opLoading}
              class="w-full py-8 border-2 border-dashed border-gray-200 dark:border-gray-600 rounded-2xl text-gray-400 dark:text-gray-500 hover:bg-gray-50 dark:bg-gray-900 dark:hover:bg-gray-700 hover:border-indigo-400 dark:hover:border-indigo-500 hover:text-indigo-600 dark:hover:text-indigo-400 transition-all flex flex-col items-center justify-center space-y-2 cursor-pointer disabled:opacity-50"
            >
              <span class="font-medium text-lg">استيراد حزمة الانتقال (.sync)</span>
              <span class="text-xs">المستخرجة من عقد الولاية</span>
            </button>
          </div>
        {:else}
          <!-- PREVIEW PANEL -->
          <div class="mt-6 bg-gray-50 dark:bg-gray-900/50 border border-gray-200 dark:border-gray-700 rounded-2xl p-5 space-y-5">
            <div class="flex justify-between items-center border-b border-gray-200 dark:border-gray-700 pb-3">
              <h3 class="font-bold text-gray-900 dark:text-white">مراجعة ترخيص الانتقال</h3>
              <button on:click={() => preview = null} class="text-xs text-indigo-600 dark:text-indigo-400 hover:underline cursor-pointer">تغيير الملف</button>
            </div>
            
            <div class="grid grid-cols-2 gap-y-4 gap-x-6 text-sm">
              <div>
                <p class="text-gray-400 dark:text-gray-500 text-xs">الانتقال المالي</p>
                <p class="font-bold text-gray-800 dark:text-gray-200">{preview.closed_year} ← {preview.opened_year}</p>
              </div>
              <div>
                <p class="text-gray-400 dark:text-gray-500 text-xs">مصدر السلطة (Wilaya)</p>
                <p class="font-bold text-gray-800 dark:text-gray-200">{preview.closure_authority_node_id}</p>
                <p class="text-[10px] text-gray-500 dark:text-gray-400">{preview.closure_authority_username}</p>
              </div>
              <div class="col-span-2">
                <p class="text-gray-400 dark:text-gray-500 text-xs">نافذة التنفيذ المصرح بها</p>
                <div class="flex flex-wrap items-center gap-2 mt-1">
                  <span class="text-[10px] font-mono bg-white dark:bg-gray-800 border dark:border-gray-600 text-gray-700 dark:text-gray-300 px-2 py-1 rounded">من: {new Date(preview.authorized_execution_window.not_before).toLocaleString()}</span>
                  <span class="text-[10px] font-mono bg-white dark:bg-gray-800 border dark:border-gray-600 text-gray-700 dark:text-gray-300 px-2 py-1 rounded">إلى: {new Date(preview.authorized_execution_window.expires_at).toLocaleString()}</span>
                </div>
              </div>
              <div class="col-span-2">
                <p class="text-gray-400 dark:text-gray-500 text-xs">معرف الانتقال (Transition ID)</p>
                <code class="text-[10px] block bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300 p-2 border dark:border-gray-600 rounded mt-1 break-all font-mono">{preview.fiscal_transition_id}</code>
              </div>
              <div class="col-span-2">
                <p class="text-gray-400 dark:text-gray-500 text-xs">بصمة الحزمة (Fingerprint)</p>
                <code class="text-[10px] block bg-white dark:bg-gray-800 p-2 border dark:border-gray-600 rounded mt-1 break-all font-mono font-bold text-indigo-700 dark:text-indigo-400">{preview.package_fingerprint}</code>
              </div>
              <div>
                <p class="text-gray-400 dark:text-gray-500 text-xs">معرف مفتاح التوقيع</p>
                <p class="font-mono text-[10px] text-gray-700 dark:text-gray-300">{preview.signing_key_id || 'DEFAULT'}</p>
              </div>
              <div class="text-left">
                 <p class="text-gray-400 dark:text-gray-500 text-xs mb-1">حالة التحقق</p>
                 {#if isInvalidSignature}
                    <AppBadge intent="danger">INVALID SIGNATURE</AppBadge>
                 {:else if isReplay}
                    <AppBadge intent="danger">REPLAY BLOCKED</AppBadge>
                 {:else if isExpired}
                    <AppBadge intent="danger">EXPIRED</AppBadge>
                 {:else if isNotYetStarted}
                    <AppBadge intent="warning">NOT YET AUTHORIZED</AppBadge>
                 {:else if preview.validation_ok}
                    <AppBadge intent="success">VALID</AppBadge>
                 {:else}
                    <AppBadge intent="danger">INVALID</AppBadge>
                 {/if}
              </div>
            </div>

            {#if !preview.validation_ok}
              <div class="mt-4">
                <AppAlert intent="danger" title="فشل التحقق التشغيلي:">
                  <ul class="list-disc pr-4 space-y-1 mt-2 text-sm">
                    {#each preview.validation_issues as issue}
                      <li>{issue}</li>
                    {/each}
                  </ul>
                </AppAlert>
              </div>
            {:else}
              <div class="space-y-4 pt-4 border-t border-gray-200 dark:border-gray-700 mt-4">
                <AppAlert intent="success">الحزمة سليمة ومصرح بتنفيذها ضمن النافذة الزمنية.</AppAlert>
                
                <AppInput
                  id="apply-confirm"
                  label="لتأكيد التنفيذ المحلي، اكتب: APPLY-FISCAL-TRANSITION"
                  bind:value={applyConfirmation}
                  placeholder="APPLY-FISCAL-TRANSITION"
                />
                <AppButton
                  variant="primary"
                  fullWidth
                  size="lg"
                  loading={$opLoading}
                  disabled={applyConfirmation !== 'APPLY-FISCAL-TRANSITION' || $opLoading}
                  on:click={handleApplyPackage}
                >
                  تنفيذ الانتقال المالي المصرح به
                </AppButton>
              </div>
            {/if}
          </div>
        {/if}
      </AppCard>
    {/if}

    {#if message}
      <AppAlert intent={message.includes('بنجاح') ? 'success' : 'danger'}>
        {message}
      </AppAlert>
    {/if}

    <!-- Fiscal Transition History Section -->
    <AppSection title="تاريخ الانتقالات المالية">
      <svelte:fragment slot="actions">
        <AppButton variant="secondary" size="sm" on:click={refreshHistory}>تحديث يدوي</AppButton>
      </svelte:fragment>

      <AppTable empty={transitionHistory.length === 0} emptyMessage="لا يوجد سجل انتقالات حالياً">
        <svelte:fragment slot="head">
          <th class="table-header">الوقت</th>
          <th class="table-header">الإجراء</th>
          <th class="table-header">السنة</th>
          <th class="table-header">الممثل</th>
          <th class="table-header">المصدر</th>
          <th class="table-header">الحالة</th>
          <th class="table-header">البصمة</th>
        </svelte:fragment>

        {#each transitionHistory as entry}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <td class="table-cell font-mono text-[10px] whitespace-nowrap text-gray-800 dark:text-gray-300">{new Date(entry.timestamp).toLocaleString('ar-DZ')}</td>
            <td class="table-cell">
               <AppBadge intent={entry.source === 'AUTHORITY' ? 'info' : 'success'} size="sm">
                {entry.action}
               </AppBadge>
            </td>
            <td class="table-cell font-bold text-gray-800 dark:text-white">{entry.fiscal_year} → {entry.next_year}</td>
            <td class="table-cell">{entry.actor}</td>
            <td class="table-cell">{entry.source}</td>
            <td class="table-cell">
              <AppBadge intent={entry.status.includes('REJECTED') || entry.status.includes('EXPIRED') ? 'danger' : 'success'} size="sm">
                {entry.status}
              </AppBadge>
            </td>
            <td class="table-cell font-mono text-[10px] text-gray-400 dark:text-gray-500" title={entry.package_fingerprint}>
              {entry.package_fingerprint ? entry.package_fingerprint.substring(0, 8) + '...' : 'N/A'}
            </td>
          </tr>
        {/each}
      </AppTable>
    </AppSection>

    <!-- Fiscal Closure Package Registry Section -->
    <AppSection title="سجل حزم إغلاق السنة المالية">
      <AppTable>
        <svelte:fragment slot="head">
          <th class="table-header">ID الانتقال</th>
          <th class="table-header">السنوات</th>
          <th class="table-header">البصمة</th>
          <th class="table-header">تاريخ التصدير</th>
          <th class="table-header">تاريخ التطبيق</th>
          <th class="table-header">الحالة</th>
          {#if nodeType === 'WILAYA'}
            <th class="table-header text-center">إجراءات الحفظ</th>
          {/if}
        </svelte:fragment>

        {#each packageRegistry as pkg}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <td class="table-cell font-mono text-[10px] break-all max-w-[120px]">{pkg.transition_id}</td>
            <td class="table-cell font-bold">{pkg.fiscal_year} → {pkg.next_year}</td>
            <td class="table-cell font-mono text-[10px] font-bold text-indigo-600 dark:text-indigo-400">{pkg.package_fingerprint.substring(0, 12)}...</td>
            <td class="table-cell whitespace-nowrap">{new Date(pkg.exported_at).toLocaleDateString('ar-DZ')}</td>
            <td class="table-cell whitespace-nowrap">{pkg.applied_at ? new Date(pkg.applied_at).toLocaleDateString('ar-DZ') : '---'}</td>
            <td class="table-cell">
              <AppBadge intent={pkg.retention_status === 'ACTIVE' ? 'success' : pkg.retention_status === 'ARCHIVED' ? 'warning' : 'neutral'} size="sm">
                {pkg.retention_status}
              </AppBadge>
            </td>
            {#if nodeType === 'WILAYA'}
              <td class="table-cell text-center space-x-1 space-x-reverse flex flex-wrap justify-center gap-1">
                <AppButton 
                  variant="primary" 
                  size="sm"
                  on:click={() => handleExportRegistryPackage(pkg.fiscal_year, pkg.next_year, pkg.exported_at, pkg.transition_id)}
                >تصدير</AppButton>
                {#if pkg.retention_status === 'ACTIVE'}
                  <AppButton 
                    variant="secondary"
                    size="sm"
                    on:click={() => { selectedPackageForRetention = pkg; targetRetentionStatus = 'ARCHIVED'; }}
                  >أرشفة</AppButton>
                  <AppButton 
                     variant="secondary"
                     size="sm"
                     on:click={() => { selectedPackageForRetention = pkg; targetRetentionStatus = 'RETIRED'; }}
                  >استبعاد</AppButton>
                {/if}
              </td>
            {/if}
          </tr>
        {/each}
      </AppTable>
    </AppSection>

    <!-- Retention Confirmation Modal -->
    <AppDialog
      open={selectedPackageForRetention !== null && targetRetentionStatus !== null}
      title="تأكيد تغيير حالة الاحتفاظ"
      on:close={() => { selectedPackageForRetention = null; targetRetentionStatus = null; retentionConfirmation = ''; }}
    >
      {#if selectedPackageForRetention}
        <div class="space-y-4">
          <p class="text-sm text-gray-600 dark:text-gray-400">
            أنت على وشك تغيير حالة الحزمة 
            <span class="font-mono font-bold dark:text-gray-200">{selectedPackageForRetention.transition_id}</span>
            إلى <span class="font-bold text-indigo-600 dark:text-indigo-400">{targetRetentionStatus}</span>.
          </p>
          
          <AppAlert intent="warning">
            <strong>ملاحظة:</strong> هذا الإجراء تشغيلي فقط ولا يؤثر على سلامة البيانات المالية، لكنه يحدد تصنيف الحزمة في الأرشيف.
          </AppAlert>

          <AppInput
            id="retention-confirm"
            label="اكتب الرمز التالي للتأكيد: {targetRetentionStatus}-PACKAGE"
            bind:value={retentionConfirmation}
            placeholder={`${targetRetentionStatus}-PACKAGE`}
          />
        </div>
      {/if}
      
      <svelte:fragment slot="actions">
        <AppButton 
          variant="secondary" 
          disabled={$opLoading}
          on:click={() => { selectedPackageForRetention = null; targetRetentionStatus = null; retentionConfirmation = ''; }}
        >إلغاء</AppButton>
        <AppButton 
          variant="danger"
          loading={$opLoading}
          disabled={retentionConfirmation !== `${targetRetentionStatus}-PACKAGE` || $opLoading}
          on:click={() => handleUpdateRetention(selectedPackageForRetention!.transition_id, targetRetentionStatus!)}
        >تأكيد التغيير</AppButton>
      </svelte:fragment>
    </AppDialog>
  </div>
</Layout>
