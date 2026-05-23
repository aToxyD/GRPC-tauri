<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
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
  import { saveFile } from '../lib/tauri';
  import { showSuccess, showError } from '../lib/notifications';
  import { formatErrorMessage } from '../lib/errors';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const reportsOp = createOperation({ scope });
  const loading = reportsOp.loading;
  const error = reportsOp.error;

  const exportOp = createOperationGuard({ scope });

  // Data
  let reports: DailyReport[] = [];
  let monthlySummaries: MonthlySummary[] = [];
  let stockMovements: StockMovement[] = [];
  let settings: Settings | null = null;
  let units: Unit[] = [];
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

  const isExporting = exportOp.loading;

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
    const suggestedName = `حالة_الوحدات_الشهرية_${currentYear}_${currentMonth}.xlsx`;
    
    const filePath = await saveFile({
      filters: [{ name: 'Excel Files', extensions: ['xlsx'] }],
      defaultPath: suggestedName
    });

    if (!filePath) return;

    await exportOp.guard(async () => {
      try {
        const result = await exportAllUnitsMonthlyStatusExcel(currentYear, currentMonth, filePath);
        
        if (result.success) {
          showSuccess(`تم التصدير بنجاح: ${result.count} وحدة مسجلة في التقرير.`);
        }
      } catch (err) {
        const errorMsg = formatErrorMessage(err);
        showError('حدث خطأ أثناء التصدير: ' + errorMsg);
      }
    });
  }

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    await reportsOp.run(async () => {
      settings = await getSettings();

      if (settings?.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
        if (units.length > 0 && !selectedUnitId) {
          selectedUnitId = units[0].id;
        }
        await loadReportsInternal();
      }
    });
  }

  async function loadReports() {
    await reportsOp.run(async () => {
      await loadReportsInternal();
    });
  }

  async function loadReportsInternal() {
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
  }

  function onUnitChange(e: Event) {
    loadReports();
  }

  function onReportTypeChange(e: Event) {
    loadReports();
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
  <div dir="rtl">
    <div class="mb-8">
      <AppCard class="bg-gradient-to-br from-blue-50 to-indigo-50 dark:from-blue-900/20 dark:to-indigo-900/20 border-blue-100 dark:border-blue-800/50">
        <div class="flex flex-col md:flex-row items-center justify-between gap-4">
          <div class="flex items-center gap-4">
            <div class="w-12 h-12 bg-civil-blue text-white rounded-full flex items-center justify-center shrink-0 shadow-sm">
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
            </div>
            <div>
              <h3 class="text-lg font-bold text-gray-800 dark:text-white">الحالة الشهرية الشاملة لجميع الوحدات</h3>
              <p class="text-sm text-gray-600 dark:text-gray-300">تصدير تقرير مجمع (Excel) يضم حالة إستهلاكات وتقارير كل الوحدات التابعة.</p>
            </div>
          </div>

          <div class="flex flex-wrap items-end gap-3 w-full md:w-auto">
            <div class="flex-1 md:w-32">
              <AppSelect
                id="exportMonth"
                label="الشهر"
                bind:value={currentMonth}
                on:change={onReportTypeChange}
                disabled={$isExporting}
              >
                {#each months as m}
                  <option value={m.value}>{m.label}</option>
                {/each}
              </AppSelect>
            </div>
            
            <div class="flex-1 md:w-24">
              <AppInput
                id="exportYear"
                label="السنة"
                type="number"
                bind:value={currentYear}
                on:change={onReportTypeChange}
                disabled={$isExporting}
              />
            </div>
            
            <AppButton 
              variant="primary"
              on:click={handleExport}
              loading={$isExporting}
              disabled={$loading || $isExporting}
            >
              <svg class="w-5 h-5 ml-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
              </svg>
              تصدير Excel
            </AppButton>
          </div>
        </div>
      </AppCard>
    </div>

    <!-- Filters -->
    <div class="mb-6">
      <AppCard padding="md">
        <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
          <AppSelect
            id="unitId"
            label="الوحدة"
            bind:value={selectedUnitId}
            on:change={onUnitChange}
          >
            <option value="">جميع الوحدات</option>
            {#each units as unit}
              <option value={unit.id}>{unit.name} ({unit.code})</option>
            {/each}
          </AppSelect>

          <AppSelect
            id="reportType"
            label="نوع التقرير"
            bind:value={selectedReportType}
            on:change={onReportTypeChange}
          >
            <option value="daily">التقارير اليومية</option>
            <option value="monthly">التقارير الشهرية</option>
            <option value="stock">حركات المخزون</option>
          </AppSelect>

          <AppSelect
            id="yearFilter"
            label="السنة"
            bind:value={currentYear}
            on:change={onReportTypeChange}
          >
            {#each Array(5) as _, i}
              <option value={new Date().getFullYear() - i}>{new Date().getFullYear() - i}</option>
            {/each}
          </AppSelect>

          {#if selectedReportType === 'daily' || selectedReportType === 'stock'}
            <AppSelect
              id="monthFilter"
              label="الشهر"
              bind:value={currentMonth}
              on:change={onReportTypeChange}
            >
              {#each Array(12) as _, i}
                <option value={i + 1}>{i + 1}</option>
              {/each}
            </AppSelect>
          {/if}
        </div>
      </AppCard>
    </div>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => reportsOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    <AppCard padding="none">
      <div class="p-4 border-b border-gray-100 dark:border-gray-700">
        <h2 class="text-lg font-semibold text-gray-800 dark:text-white">
          {#if selectedReportType === 'daily'}التقارير اليومية
          {:else if selectedReportType === 'monthly'}الملخصات الشهرية
          {:else if selectedReportType === 'stock'}حركات المخزون
          {/if}
        </h2>
      </div>

      {#if $loading}
        <AppLoadingState message="جاري التحميل..." />
      {:else}
        {#if selectedReportType === 'monthly'}
          <AppTable empty={monthlySummaries.length === 0}>
            <svelte:fragment slot="empty">
              <AppEmptyState
                title="لا توجد تقارير شهرية"
                icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svelte:fragment>
            <svelte:fragment slot="head">
              <th class="table-header">الشهر/السنة</th>
              <th class="table-header">عدد الأيام</th>
              <th class="table-header">المستفيدون</th>
              <th class="table-header">القيمة الإجمالية</th>
              <th class="table-header">المعدل اليومي</th>
              <th class="table-header text-left">الإجراءات</th>
            </svelte:fragment>

            {#each monthlySummaries as summary}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="table-cell font-medium">{summary.month}/{summary.year}</td>
                <td class="table-cell">{summary.report_count}</td>
                <td class="table-cell">{summary.total_beneficiaries}</td>
                <td class="table-cell font-medium">{summary.total_consumption_value.toFixed(2)} دج</td>
                <td class="table-cell">{summary.daily_average.toFixed(2)} دج</td>
                <td class="table-cell text-left">
                  <AppButton variant="ghost" size="sm" ariaLabel="عرض التفاصيل" on:click={() => viewMonthlySummary(summary)}>
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                    </svg>
                  </AppButton>
                </td>
              </tr>
            {/each}
          </AppTable>

        {:else if selectedReportType === 'stock'}
          <AppTable empty={stockMovements.length === 0}>
            <svelte:fragment slot="empty">
              <AppEmptyState
                title="لا توجد حركات مخزون"
                icon="M8 7v8a2 2 0 002 2h6M8 7V5a2 2 0 012-2h4.586a1 1 0 01.707.293l4.414 4.414a1 1 0 01.293.707V15a2 2 0 01-2 2h-2M8 7H6a2 2 0 00-2 2v10a2 2 0 002 2h8a2 2 0 002-2v-2"
              />
            </svelte:fragment>
            <svelte:fragment slot="head">
              <th class="table-header">التاريخ</th>
              <th class="table-header">المنتج</th>
              <th class="table-header">نوع الحركة</th>
              <th class="table-header">الكمية</th>
              <th class="table-header">تكلفة الاستحواذ</th>
              <th class="table-header">الرصيد بعد</th>
              <th class="table-header text-left">الإجراءات</th>
            </svelte:fragment>

            {#each stockMovements as movement}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="table-cell font-medium">{formatDate(movement.timestamp)}</td>
                <td class="table-cell">{movement.product_name || movement.product_id}</td>
                <td class="table-cell">
                  {#if movement.movement_type === 'IN'}
                    <AppBadge intent="success">دخول</AppBadge>
                  {:else if movement.movement_type === 'OUT'}
                    <AppBadge intent="danger">خروج</AppBadge>
                  {:else if movement.movement_type === 'OPENING'}
                    <AppBadge intent="info">افتتاحي</AppBadge>
                  {:else}
                    <AppBadge intent="neutral">{movement.movement_type}</AppBadge>
                  {/if}
                </td>
                <td class="table-cell">{movement.quantity.toFixed(2)}</td>
                <td class="table-cell">
                  {movement.unit_cost !== null && movement.unit_cost !== undefined ? movement.unit_cost.toFixed(2) + " دج" : "-"}
                </td>
                <td class="table-cell">{movement.balance_after.toFixed(2)}</td>
                <td class="table-cell text-left">
                  <AppButton variant="ghost" size="sm" ariaLabel="عرض التفاصيل" on:click={() => viewStockMovement(movement)}>
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                    </svg>
                  </AppButton>
                </td>
              </tr>
            {/each}
          </AppTable>

        {:else}
          <AppTable empty={reports.length === 0}>
            <svelte:fragment slot="empty">
              <AppEmptyState
                title="لا توجد تقارير يومية"
                icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
              />
            </svelte:fragment>
            <svelte:fragment slot="head">
              <th class="table-header">التاريخ</th>
              <th class="table-header">الوحدة</th>
              <th class="table-header">المستفيدون</th>
              <th class="table-header">التكلفة اليومية</th>
              <th class="table-header">المعدل اليومي</th>
              <th class="table-header text-left">الإجراءات</th>
            </svelte:fragment>

            {#each reports as report}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="table-cell font-medium">{formatDate(report.date)}</td>
                <td class="table-cell">{report.unit_id || '-'}</td>
                <td class="table-cell">{report.total_daily_beneficiaries}</td>
                <td class="table-cell font-medium">{report.total_daily_cost.toFixed(2)} دج</td>
                <td class="table-cell">{report.total_daily_average.toFixed(2)} دج</td>
                <td class="table-cell text-left">
                  <AppButton variant="ghost" size="sm" ariaLabel="عرض التقرير" on:click={() => viewDetails(report)}>
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                    </svg>
                  </AppButton>
                </td>
              </tr>
            {/each}
          </AppTable>
        {/if}
      {/if}
    </AppCard>
  </div>
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
