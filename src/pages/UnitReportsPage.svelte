<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatErrorMessage } from '../lib/errors';
  import {
    listDailyReports, getDailyReport, exportDailyReportPackage, exportMonthlySummaryPackage,
    getMonthlySummary, getSettings, saveFile, listFiscalYears
  } from '../lib/tauri';
  import type { DailyReport, DailyReportResult, MonthlySummary, Settings } from '../lib/types';
  import DailyReportModal from '../components/reports/DailyReportModal.svelte';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  const reportsOp = createOperation({ scope });
  const loading = reportsOp.loading;
  const error = reportsOp.error;

  // Data
  let reports: DailyReport[] = [];
  let settings: Settings | null = null;
  let success = '';
  let selectedReport: DailyReportResult | null = null;
  let viewingDetails = false;

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m));
  onDestroy(() => scope.dispose());

  // Fiscal year / month filtering
  let fiscalYears: number[] = [];
  let selectedYear: number = new Date().getFullYear();
  let selectedMonth: number = 0; // 0 = all months (full fiscal year)
  let currentMonth = new Date().getMonth() + 1;
  let currentYear = new Date().getFullYear();
  let monthlySummary: MonthlySummary | null = null;

  function selectedMonthApi(): number | undefined {
    return selectedMonth > 0 ? selectedMonth : undefined;
  }

  const MONTH_OPTIONS = [
    { value: 0, label: 'الكل (السنة المالية)' },
    { value: 1, label: 'جانفي' },
    { value: 2, label: 'فيفري' },
    { value: 3, label: 'مارس' },
    { value: 4, label: 'أفريل' },
    { value: 5, label: 'ماي' },
    { value: 6, label: 'جوان' },
    { value: 7, label: 'جويلية' },
    { value: 8, label: 'أوت' },
    { value: 9, label: 'سبتمبر' },
    { value: 10, label: 'أكتوبر' },
    { value: 11, label: 'نوفمبر' },
    { value: 12, label: 'ديسمبر' },
  ] as const;

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    await reportsOp.run(async () => {
      settings = await getSettings();
      fiscalYears = await listFiscalYears();

      if (settings) {
        selectedYear = settings.current_year || fiscalYears[0] || new Date().getFullYear();
        currentYear = selectedYear;
      }
      await loadFilteredDataInner();
    });
  }

  async function loadFilteredDataInner() {
    const apiMonth = selectedMonthApi();
    [reports, monthlySummary] = await Promise.all([
      listDailyReports(undefined, undefined, selectedYear, apiMonth),
      getMonthlySummary(selectedYear, apiMonth)
    ]);
  }

  async function loadFilteredData() {
    await reportsOp.run(loadFilteredDataInner);
  }

  function applyFilter() {
    loadFilteredData();
  }

  async function viewDetails(report: DailyReport) {
    try {
      const result = await getDailyReport(report.id);
      selectedReport = result;
      viewingDetails = true;
    } catch (e) {
      reportsOp.error.set('خطأ في تحميل التفاصيل: ' + formatErrorMessage(e));
    }
  }

  function closeDetails() {
    selectedReport = null;
    viewingDetails = false;
  }

  async function exportDailyReport(report: DailyReport) {
    const filePath = await saveFile({
      filters: [{ name: 'حزمة المزامنة', extensions: ['sync'] }],
      defaultPath: `تقرير_الإستهلاك_اليومي_${settings?.unit_code || 'UNIT'}_${report.date}.sync`
    });
    if (filePath) {
      await reportsOp.run(async () => {
        const result = await exportDailyReportPackage(report.id, filePath);
        if (result.success) {
          setSuccessWithTimeout(`تم تصدير التقرير: ${result.file_path}`);
        }
      });
    }
  }

  async function exportMonthly() {
    const label = selectedMonth ? `شهري_${selectedMonth}` : 'سنوي';
    const filePath = await saveFile({
      filters: [{ name: 'حزمة المزامنة', extensions: ['sync'] }],
      defaultPath: `تقرير_${label}_${settings?.unit_code || 'UNIT'}_${selectedYear}.sync`
    });
    if (filePath) {
      await reportsOp.run(async () => {
        const exportMonth = selectedMonthApi() ?? 1;
        const result = await exportMonthlySummaryPackage(selectedYear, exportMonth, filePath);
        if (result.success) {
          setSuccessWithTimeout(`تم تصدير التقرير: ${result.record_count} سجل`);
        }
      });
    }
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString('fr-FR', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric'
    });
  }
</script>

<Layout nodeType="UNIT" title="التقارير والتصدير" subtitle="عرض وتصدير تقارير الاستهلاك">
  <div dir="rtl">
    <!-- Fiscal Year / Month Filter Bar -->
    {#if fiscalYears.length > 0}
      <div class="mb-6 flex flex-wrap gap-4 items-end bg-white dark:bg-gray-800 p-4 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
        <div>
          <label class="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-2">السنة المالية</label>
          <select
            class="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer"
            bind:value={selectedYear}
          >
            {#each fiscalYears as y}
              <option value={y}>{y}</option>
            {/each}
          </select>
        </div>
        <div>
          <label class="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-2">الشهر</label>
          <select
            class="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer"
            bind:value={selectedMonth}
          >
            {#each MONTH_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </div>
        <AppButton variant="primary" on:click={applyFilter}>تطبيق</AppButton>
      </div>
    {/if}

    <!-- Export Button -->
    <div class="mb-8 flex justify-end">
      <AppButton variant="primary" on:click={exportMonthly} ariaLabel="تصدير حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد">
        <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
        </svg>
        تصدير الشهري
      </AppButton>
    </div>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => reportsOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => success = ''}>{success}</AppAlert>
      </div>
    {/if}

    {#if $loading}
      <AppLoadingState message="...جارٍ التحميل" />
    {:else}
      <!-- Monthly Summary Card -->
      {#if monthlySummary}
        <AppCard class="mb-6 bg-gradient-to-r from-blue-50/50 to-blue-100/30 dark:from-blue-900/10 dark:to-blue-900/5">
          <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">ملخص {selectedMonth ? `الشهر ${selectedMonth}` : 'السنة المالية'} ({selectedYear})</h2>
          <div class="grid grid-cols-2 md:grid-cols-3 gap-4">
            <div class="text-center p-4 bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
              <p class="text-sm text-gray-500 dark:text-gray-400">المستفيدون</p>
              <p class="text-xl font-bold text-civil-blue dark:text-blue-400 mt-1">{monthlySummary.total_beneficiaries}</p>
            </div>
            <div class="text-center p-4 bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
              <p class="text-sm text-gray-500 dark:text-gray-400">القيمة الإجمالية</p>
              <p class="text-xl font-bold text-civil-blue dark:text-blue-400 mt-1">{monthlySummary.total_consumption_value.toFixed(2)} دج</p>
            </div>
            <div class="text-center p-4 bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
              <p class="text-sm text-gray-500 dark:text-gray-400">المعدل اليومي</p>
              <p class="text-xl font-bold text-civil-blue dark:text-blue-400 mt-1">{monthlySummary.daily_average.toFixed(2)} دج</p>
            </div>
            <div class="text-center p-4 bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
              <p class="text-sm text-gray-500 dark:text-gray-400">متوسط الفطور</p>
              <p class="text-lg font-bold text-gray-800 dark:text-gray-100 mt-1">{monthlySummary.breakfast_average.toFixed(2)} دج</p>
            </div>
            <div class="text-center p-4 bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
              <p class="text-sm text-gray-500 dark:text-gray-400">متوسط الغداء</p>
              <p class="text-lg font-bold text-gray-800 dark:text-gray-100 mt-1">{monthlySummary.lunch_average.toFixed(2)} دج</p>
            </div>
            <div class="text-center p-4 bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-100 dark:border-gray-700">
              <p class="text-sm text-gray-500 dark:text-gray-400">متوسط العشاء</p>
              <p class="text-lg font-bold text-gray-800 dark:text-gray-100 mt-1">{monthlySummary.dinner_average.toFixed(2)} دج</p>
            </div>
          </div>
        </AppCard>
      {/if}

      <!-- Reports List -->
      <AppCard padding="none">
        <div class="p-4 border-b border-gray-100 dark:border-gray-700">
          <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100">التقارير اليومية</h2>
        </div>

        <AppTable empty={reports.length === 0}>
          <svelte:fragment slot="empty">
            <AppEmptyState
              title="لا يوجد تقارير مسجلة"
              description="أضف تقارير الاستهلاك اليومي لعرضها هنا"
              icon="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            >
              <svelte:fragment slot="action">
                <a href="#/unit/consumption">
                  <AppButton variant="primary">إنشاء تقرير</AppButton>
                </a>
              </svelte:fragment>
            </AppEmptyState>
          </svelte:fragment>
          
          <svelte:fragment slot="head">
            <th class="table-header">التاريخ</th>
            <th class="table-header">المستفيدون</th>
            <th class="table-header">التكلفة اليومية</th>
            <th class="table-header">المعدل اليومي</th>
            <th class="table-header text-left">الإجراءات</th>
          </svelte:fragment>

          {#each reports as report}
            <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
              <td class="table-cell font-medium">{formatDate(report.date)}</td>
              <td class="table-cell">{report.total_daily_beneficiaries}</td>
              <td class="table-cell font-medium">{report.total_daily_cost.toFixed(2)} دج</td>
              <td class="table-cell">{report.total_daily_average.toFixed(2)} دج</td>
              <td class="table-cell text-left space-x-2 space-x-reverse">
                <AppButton variant="ghost" size="sm" class="text-civil-blue hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300" on:click={() => viewDetails(report)} ariaLabel="معاينة التقرير">
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                  </svg>
                </AppButton>
                <AppButton variant="ghost" size="sm" class="text-green-600 hover:text-green-800 dark:text-green-400 dark:hover:text-green-300" on:click={() => exportDailyReport(report)} ariaLabel="تصدير حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد">
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
                  </svg>
                </AppButton>
              </td>
            </tr>
          {/each}
        </AppTable>
      </AppCard>

      <div class="mt-6">
        <AppAlert intent="info" title="التصدير للولاية">
          <span class="text-sm">التصدير اليومي والشهري عبر حزمة المزامنة (.sync) المؤمنة. انقل الملفات عبر USB أو البريد الإلكتروني.</span>
        </AppAlert>
      </div>
    {/if}
  </div>
</Layout>

{#if viewingDetails && selectedReport}
  <DailyReportModal selectedReport={selectedReport} on:close={closeDetails} />
{/if}
