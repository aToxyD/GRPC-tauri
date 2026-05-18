<script lang="ts">
  import { onMount } from 'svelte';
  import {
    listDailyReports, getDailyReport, exportDailyReportPackage, exportMonthlySummaryPackage,
    getMonthlySummary, getSettings
  } from '../lib/tauri';
  import { save } from '@tauri-apps/plugin-dialog';
  import type { DailyReport, DailyReportResult, MonthlySummary, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  // Data
  let reports: DailyReport[] = [];
  let settings: Settings | null = null;
  let loading = true;
  let error = '';
  let success = '';
  let selectedReport: DailyReportResult | null = null;
  let viewingDetails = false;

  // Monthly summary
  let currentMonth = new Date().getMonth() + 1;
  let currentYear = new Date().getFullYear();
  let monthlySummary: MonthlySummary | null = null;

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    try {
      loading = true;
      settings = await getSettings();

      if (settings) {
        currentYear = settings.current_year;
        [reports, monthlySummary] = await Promise.all([
          listDailyReports(),
          getMonthlySummary(currentYear, currentMonth)
        ]);
      }
    } catch (e) {
      error = 'خطأ في التحميل: ' + String(e);
    } finally {
      loading = false;
    }
  }

  async function viewDetails(report: DailyReport) {
    try {
      const result = await getDailyReport(report.id);
      selectedReport = result;
      viewingDetails = true;
    } catch (e) {
      error = 'خطأ في تحميل التفاصيل: ' + String(e);
    }
  }

  function closeDetails() {
    selectedReport = null;
    viewingDetails = false;
  }

  async function exportDailyReport(report: DailyReport) {
    try {
      const filePath = await save({
        filters: [{ name: 'حزمة المزامنة', extensions: ['sync'] }],
        defaultPath: `تقرير_الإستهلاك_اليومي_${settings?.unit_code || 'UNIT'}_${report.date}.sync`
      });
      if (filePath) {
        const result = await exportDailyReportPackage(report.id, filePath);
        if (result.success) {
          success = `تم تصدير التقرير: ${result.file_path}`;
          setTimeout(() => success = '', 3000);
        }
      }
    } catch (e) {
      error = 'خطأ في التصدير: ' + String(e);
    }
  }

  async function exportMonthly() {
    try {
      const filePath = await save({
        filters: [{ name: 'حزمة المزامنة', extensions: ['sync'] }],
        defaultPath: `تقرير_الإستهلاك_الشهري_${settings?.unit_code || 'UNIT'}_${currentYear}_${currentMonth}.sync`
      });
      if (filePath) {
        const result = await exportMonthlySummaryPackage(currentYear, currentMonth, filePath);
        if (result.success) {
          success = `تم تصدير التقرير الشهري: ${result.record_count} سجل`;
          setTimeout(() => success = '', 3000);
        }
      }
    } catch (e) {
      error = 'خطأ في التصدير: ' + String(e);
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
  <!-- Export Button -->
  <div class="mb-8">
    <div class="flex items-center justify-end">
      <button on:click={exportMonthly} class="btn-primary flex items-center gap-2" title="تصدير حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
        </svg>
        <span>تصدير الشهري</span>
      </button>
    </div>
  </div>

  {#if error}
    <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
      {error}
    </div>
  {/if}

  {#if success}
    <div class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm">
      {success}
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center justify-center py-12">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"></div>
    </div>
  {:else}
    <!-- Monthly Summary Card -->
    {#if monthlySummary}
      <div class="card mb-6 bg-gradient-to-r from-civil-blue/5 to-civil-blue/10">
        <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">ملخص الشهر ({currentMonth}/{currentYear})</h2>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div class="text-center p-3 bg-white dark:bg-gray-800 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">الموظفون</p>
            <p class="text-xl font-bold text-civil-blue">{monthlySummary.total_personnel}</p>
          </div>
          <div class="text-center p-3 bg-white dark:bg-gray-800 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">الضيوف</p>
            <p class="text-xl font-bold text-civil-blue">{monthlySummary.total_guests}</p>
          </div>
          <div class="text-center p-3 bg-white dark:bg-gray-800 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">القيمة الإجمالية</p>
            <p class="text-xl font-bold text-civil-blue">{monthlySummary.total_consumption_value.toFixed(2)} دج</p>
          </div>
          <div class="text-center p-3 bg-white dark:bg-gray-800 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">المعدل المتوسط</p>
            <p class="text-xl font-bold text-civil-blue">{monthlySummary.average_meal_rate.toFixed(2)} دج</p>
          </div>
        </div>
      </div>
    {/if}

    <!-- Reports List -->
    <div class="card">
      <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">التقارير اليومية</h2>

      {#if reports.length === 0}
        <div class="text-center py-12 text-gray-500 dark:text-gray-400">
          <svg class="w-16 h-16 mx-auto mb-4 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
          </svg>
          <p class="text-lg">لا يوجد تقارير مسجلة</p>
          <a href="#/unit/consumption" class="text-civil-blue hover:underline mt-2 inline-block">إنشاء تقرير</a>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">التاريخ</th>
                <th class="table-header">الموظفون</th>
                <th class="table-header">الضيوف</th>
                <th class="table-header">التكلفة الإجمالية</th>
                <th class="table-header">المعدل/وجبة</th>
                <th class="table-header text-right">الإجراءات</th>
              </tr>
            </thead>
            <tbody>
              {#each reports as report}
                <tr class="hover:bg-gray-50 dark:bg-gray-900">
                  <td class="table-cell font-medium">{formatDate(report.date)}</td>
                  <td class="table-cell">{report.personnel_count}</td>
                  <td class="table-cell">{report.guest_count}</td>
                  <td class="table-cell font-medium">{report.total_meals_cost.toFixed(2)} دج</td>
                  <td class="table-cell">{report.actual_meal_rate.toFixed(2)} دج</td>
                  <td class="table-cell text-right">
                    <button
                      on:click={() => viewDetails(report)}
                      class="text-civil-blue hover:text-civil-blue-dark mr-3"
                      title="معاينة التقرير"
                    >
                      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                      </svg>
                    </button>
                    <button
                      on:click={() => exportDailyReport(report)}
                      class="text-green-600 hover:text-green-700"
                      title="تصدير حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد"
                    >
                      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
                      </svg>
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <div class="mt-6 card bg-blue-50 dark:bg-blue-900/20 border-blue-200">
      <h3 class="font-semibold text-blue-800 dark:text-blue-300 mb-2">التصدير للولاية</h3>
      <p class="text-sm text-blue-700">
        التصدير اليومي والشهري عبر حزمة المزامنة (.sync) المؤمنة.
        انقل الملفات عبر USB أو البريد الإلكتروني.
      </p>
    </div>
  {/if}
</Layout>

<!-- Report Details Modal -->
{#if viewingDetails && selectedReport}
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-3xl mx-4 max-h-[90vh] overflow-hidden">
      <div class="p-6 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
        <div>
          <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">تفاصيل التقرير</h2>
          <p class="text-sm text-gray-500 dark:text-gray-400">{formatDate(selectedReport.report.date)}</p>
        </div>
        <button on:click={closeDetails} class="text-gray-400 hover:text-gray-600 dark:text-gray-400" aria-label="إغلاق">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
          </svg>
        </button>
      </div>
      <div class="p-6 overflow-y-auto max-h-[60vh]">
        <div class="grid grid-cols-4 gap-4 mb-6">
          <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">الموظفون</p>
            <p class="text-lg font-bold">{selectedReport.report.personnel_count}</p>
          </div>
          <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">الضيوف</p>
            <p class="text-lg font-bold">{selectedReport.report.guest_count}</p>
          </div>
          <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">التكلفة الإجمالية</p>
            <p class="text-lg font-bold text-civil-blue">{selectedReport.report.total_meals_cost.toFixed(2)} دج</p>
          </div>
          <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
            <p class="text-sm text-gray-500 dark:text-gray-400">المعدل/وجبة</p>
            <p class="text-lg font-bold text-civil-blue">{selectedReport.report.actual_meal_rate.toFixed(2)} دج</p>
          </div>
        </div>
        <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-3">المنتجات المستهلكة</h3>
        <table class="w-full">
          <thead>
            <tr>
              <th class="table-header">المنتج</th>
              <th class="table-header text-right">الكمية</th>
              <th class="table-header text-right">سعر الوحدة</th>
              <th class="table-header text-right">الإجمالي</th>
            </tr>
          </thead>
          <tbody>
            {#each selectedReport.items as item}
              <tr>
                <td class="table-cell">{item.product_name}</td>
                <td class="table-cell text-right">{item.quantity.toFixed(2)}</td>
                <td class="table-cell text-right">{item.unit_price.toFixed(2)} دج</td>
                <td class="table-cell text-right font-medium">{item.total_cost.toFixed(2)} دج</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </div>
{/if}
