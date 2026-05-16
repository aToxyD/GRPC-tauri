<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getDailyReport, listUnits, listWilayaReports, getSettings
  } from '../lib/tauri';
  import type { ReportType } from '../lib/tauri';
  import type { DailyReport, DailyReportResult, MonthlySummary, Settings, Unit, StockMovement, WilayaReportList } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import DailyReportModal from '../components/reports/DailyReportModal.svelte';
  import MonthlySummaryModal from '../components/reports/MonthlySummaryModal.svelte';
  import StockMovementModal from '../components/reports/StockMovementModal.svelte';
  import { exportAllUnitsMonthlyStatusExcel } from '../lib/tauri';
  import { save } from '@tauri-apps/plugin-dialog';
  import { showSuccess, showError } from '../lib/notifications';

  // Data
  let reports: DailyReport[] = [];
  let monthlySummaries: MonthlySummary[] = [];
  let stockMovements: StockMovement[] = [];
  let settings: Settings | null = null;
  let units: Unit[] = [];
  let loading = true;
  let error = '';
  let selectedReport: DailyReportResult | null = null;
  let selectedStockMovement: StockMovement | null = null;
  let selectedMonthlySummary: MonthlySummary | null = null;
  let viewingDetails = false;
  let viewingStockMovement = false;
  let viewingMonthlySummary = false;

  // Filters
  let selectedUnitId: string = '';
  let selectedReportType: ReportType = 'daily';
  let currentMonth = new Date().getMonth() + 1;
  let currentYear = new Date().getFullYear();

  let isExporting = false;

  const months = [
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
    { value: 12, label: 'ديسمبر' }
  ];

  async function handleExport() {
    try {
      const suggestedName = `حالة_الوحدات_الشهرية_${currentYear}_${currentMonth}.xlsx`;
      
      const filePath = await save({
        filters: [{ name: 'Excel Files', extensions: ['xlsx'] }],
        defaultPath: suggestedName
      });

      if (!filePath) return;

      isExporting = true;
      const result = await exportAllUnitsMonthlyStatusExcel(currentYear, currentMonth, filePath);
      
      if (result.success) {
        showSuccess(`تم التصدير بنجاح: ${result.count} وحدة مسجلة في التقرير.`);
      }
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      showError('حدث خطأ أثناء التصدير: ' + errorMsg);
    } finally {
      isExporting = false;
    }
  }

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    try {
      loading = true;
      settings = await getSettings();

      if (settings?.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
        if (units.length > 0 && !selectedUnitId) {
          selectedUnitId = units[0].id;
        }
        await loadReports();
      }
    } catch (e) {
      error = 'خطأ في التحميل: ' + String(e);
    } finally {
      loading = false;
    }
  }

  async function loadReports() {
    try {
      loading = true;
      error = '';

      const result = await listWilayaReports(
        selectedUnitId || null,
        selectedReportType,
        currentYear,
        selectedReportType === 'monthly' ? undefined : currentMonth
      );

      // Reset all report types
      reports = [];
      monthlySummaries = [];
      stockMovements = [];

      if (result.type === 'Daily') {
        reports = result.data as DailyReport[];
      } else if (result.type === 'Monthly') {
        monthlySummaries = result.data as MonthlySummary[];
      } else if (result.type === 'Stock') {
        stockMovements = result.data as StockMovement[];
      }
    } catch (e) {
      error = 'خطأ في تحميل التقارير: ' + String(e);
    } finally {
      loading = false;
    }
  }

  function onUnitChange() {
    loadReports();
  }

  function onReportTypeChange() {
    loadReports();
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

  function viewMonthlySummary(summary: MonthlySummary) {
    selectedMonthlySummary = summary;
    viewingMonthlySummary = true;
  }

  function closeMonthlySummary() {
    selectedMonthlySummary = null;
    viewingMonthlySummary = false;
  }

  function viewStockMovement(movement: StockMovement) {
    selectedStockMovement = movement;
    viewingStockMovement = true;
  }

  function closeStockMovement() {
    selectedStockMovement = null;
    viewingStockMovement = false;
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

<Layout nodeType="WILAYA" title="التقارير" subtitle="عرض تقارير الوحدات">
  
  <div class="card bg-gradient-to-br from-blue-50 to-indigo-50 border border-blue-100 mb-8">
    <div class="flex flex-col md:flex-row items-center justify-between gap-4">
      <div class="flex items-center gap-4">
        <div class="w-12 h-12 bg-civil-blue text-white rounded-full flex items-center justify-center shrink-0 shadow-sm">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
        </div>
        <div>
          <h3 class="text-lg font-bold text-gray-800">الحالة الشهرية الشاملة لجميع الوحدات</h3>
          <p class="text-sm text-gray-600">تصدير تقرير مجمع (Excel) يضم حالة إستهلاكات وتقارير كل الوحدات التابعة.</p>
        </div>
      </div>

      <div class="flex items-center gap-3 w-full md:w-auto">
        <select bind:value={currentMonth} on:change={onReportTypeChange} class="input flex-1 md:w-32 py-2" disabled={isExporting}>
          {#each months as m}
            <option value={m.value}>{m.label}</option>
          {/each}
        </select>
        
        <input type="number" bind:value={currentYear} on:change={onReportTypeChange} class="input flex-1 md:w-24 py-2" disabled={isExporting} />
        
        <button 
          class="btn-primary flex items-center gap-2 whitespace-nowrap"
          on:click={handleExport}
          disabled={isExporting}
        >
          {#if isExporting}
            <svg class="animate-spin h-5 w-5 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            جاري التصدير...
          {:else}
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
            تصدير Excel
          {/if}
        </button>
      </div>
    </div>
  </div>

  <!-- Filters -->
  <div class="mb-6 p-4 bg-gray-50 rounded-lg">
    <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
      <label class="block">
        <span class="block text-sm font-medium text-gray-700 mb-1">الوحدة</span>
        <select
          bind:value={selectedUnitId}
          on:change={onUnitChange}
          class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-civil-blue focus:border-civil-blue"
        >
          <option value="">جميع الوحدات</option>
          {#each units as unit}
            <option value={unit.id}>{unit.name} ({unit.code})</option>
          {/each}
        </select>
      </label>

      <label class="block">
        <span class="block text-sm font-medium text-gray-700 mb-1">نوع التقرير</span>
        <select
          bind:value={selectedReportType}
          on:change={onReportTypeChange}
          class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-civil-blue focus:border-civil-blue"
        >
          <option value="daily">التقارير اليومية</option>
          <option value="monthly">التقارير الشهرية</option>
          <option value="stock">حركات المخزون</option>
        </select>
      </label>

      <label class="block">
        <span class="block text-sm font-medium text-gray-700 mb-1">السنة</span>
        <select
          bind:value={currentYear}
          on:change={onReportTypeChange}
          class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-civil-blue focus:border-civil-blue"
        >
          {#each Array(5) as _, i}
            <option value={new Date().getFullYear() - i}>{new Date().getFullYear() - i}</option>
          {/each}
        </select>
      </label>

      {#if selectedReportType === 'daily' || selectedReportType === 'stock'}
        <label class="block">
          <span class="block text-sm font-medium text-gray-700 mb-1">الشهر</span>
          <select
            bind:value={currentMonth}
            on:change={onReportTypeChange}
            class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-civil-blue focus:border-civil-blue"
          >
            {#each Array(12) as _, i}
              <option value={i + 1}>{i + 1}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>
  </div>

  {#if error}
    <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
      {error}
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center justify-center py-12">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"></div>
    </div>
  {:else}
    <div class="card">
      <h2 class="text-lg font-semibold text-gray-800 mb-4">
        {#if selectedReportType === 'daily'}التقارير اليومية
        {:else if selectedReportType === 'monthly'}الملخصات الشهرية
        {:else if selectedReportType === 'stock'}حركات المخزون
        {/if}
      </h2>

      {#if selectedReportType === 'daily' && reports.length === 0}
        <div class="text-center py-12 text-gray-500">
          <p class="text-lg">لا يوجد تقارير يومية</p>
        </div>
      {:else if selectedReportType === 'monthly' && monthlySummaries.length === 0}
        <div class="text-center py-12 text-gray-500">
          <p class="text-lg">لا يوجد تقارير شهرية</p>
        </div>
      {:else if selectedReportType === 'stock' && stockMovements.length === 0}
        <div class="text-center py-12 text-gray-500">
          <p class="text-lg">لا يوجد حركات مخزون</p>
        </div>
      {:else}
        <div class="overflow-x-auto">
          {#if selectedReportType === 'monthly'}
            <table class="w-full">
              <thead>
                <tr>
                  <th class="table-header">الشهر/السنة</th>
                  <th class="table-header">عدد التقارير</th>
                  <th class="table-header">الموظفون</th>
                  <th class="table-header">الضيوف</th>
                  <th class="table-header">القيمة الإجمالية</th>
                  <th class="table-header">المعدل/وجبة</th>
                  <th class="table-header text-right">الإجراءات</th>
                </tr>
              </thead>
              <tbody>
                {#each monthlySummaries as summary}
                  <tr class="hover:bg-gray-50">
                    <td class="table-cell font-medium">{summary.month}/{summary.year}</td>
                    <td class="table-cell">{summary.report_count}</td>
                    <td class="table-cell">{summary.total_personnel}</td>
                    <td class="table-cell">{summary.total_guests}</td>
                    <td class="table-cell font-medium">{summary.total_consumption_value.toFixed(2)} دج</td>
                    <td class="table-cell">{summary.average_meal_rate.toFixed(2)} دج</td>
                    <td class="table-cell text-right">
                      <button on:click={() => viewMonthlySummary(summary)} class="text-civil-blue hover:text-civil-blue-dark" title="عرض التفاصيل">
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                        </svg>
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else if selectedReportType === 'stock'}
            <table class="w-full">
              <thead>
                <tr>
                  <th class="table-header">التاريخ</th>
                  <th class="table-header">المنتج</th>
                  <th class="table-header">نوع الحركة</th>
                  <th class="table-header">الكمية</th>
                  <th class="table-header">الرصيد بعد</th>
                  <th class="table-header text-right">الإجراءات</th>
                </tr>
              </thead>
              <tbody>
                {#each stockMovements as movement}
                  <tr class="hover:bg-gray-50">
                    <td class="table-cell font-medium">{formatDate(movement.timestamp)}</td>
                    <td class="table-cell">{movement.product_name || movement.product_id}</td>
                    <td class="table-cell">
                      <span class="px-2 py-1 rounded text-xs font-medium"
                        class:bg-green-100={movement.movement_type === 'IN'}
                        class:text-green-800={movement.movement_type === 'IN'}
                        class:bg-red-100={movement.movement_type === 'OUT'}
                        class:text-red-800={movement.movement_type === 'OUT'}
                        class:bg-blue-100={movement.movement_type === 'OPENING'}
                        class:text-blue-800={movement.movement_type === 'OPENING'}
                      >
                        {#if movement.movement_type === 'IN'}دخول
                        {:else if movement.movement_type === 'OUT'}خروج
                        {:else if movement.movement_type === 'OPENING'}افتتاحي
                        {/if}
                      </span>
                    </td>
                    <td class="table-cell">{movement.quantity.toFixed(2)}</td>
                    <td class="table-cell">{movement.balance_after.toFixed(2)}</td>
                    <td class="table-cell text-right">
                      <button on:click={() => viewStockMovement(movement)} class="text-civil-blue hover:text-civil-blue-dark" title="عرض التفاصيل">
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                        </svg>
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else}
            <table class="w-full">
              <thead>
                <tr>
                  <th class="table-header">التاريخ</th>
                  <th class="table-header">الوحدة</th>
                  <th class="table-header">الموظفون</th>
                  <th class="table-header">الضيوف</th>
                  <th class="table-header">التكلفة الإجمالية</th>
                  <th class="table-header">المعدل/وجبة</th>
                  <th class="table-header text-right">الإجراءات</th>
                </tr>
              </thead>
              <tbody>
                {#each reports as report}
                  <tr class="hover:bg-gray-50">
                    <td class="table-cell font-medium">{formatDate(report.date)}</td>
                    <td class="table-cell">{report.unit_id || '-'}</td>
                    <td class="table-cell">{report.personnel_count}</td>
                    <td class="table-cell">{report.guest_count}</td>
                    <td class="table-cell font-medium">{report.total_meals_cost.toFixed(2)} دج</td>
                    <td class="table-cell">{report.actual_meal_rate.toFixed(2)} دج</td>
                    <td class="table-cell text-right">
                      <button on:click={() => viewDetails(report)} class="text-civil-blue hover:text-civil-blue-dark" title="عرض التقرير">
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                        </svg>
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</Layout>

<!-- Modals -->
{#if viewingDetails && selectedReport}
  <DailyReportModal 
    {selectedReport} 
    on:close={closeDetails} 
  />
{/if}

{#if viewingMonthlySummary && selectedMonthlySummary}
  <MonthlySummaryModal 
    summary={selectedMonthlySummary} 
    on:close={closeMonthlySummary}
    on:viewDaily={(e) => {
      currentMonth = e.detail.month;
      currentYear = e.detail.year;
      selectedReportType = 'daily';
      closeMonthlySummary();
      loadReports();
    }}
  />
{/if}

{#if viewingStockMovement && selectedStockMovement}
  <StockMovementModal 
    movement={selectedStockMovement} 
    on:close={closeStockMovement} 
  />
{/if}
