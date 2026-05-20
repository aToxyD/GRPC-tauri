<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    listUnits, getSettings,
    computeUnitInventorySnapshot, getUnitInventoryView,
    getAvailableReportMonths, exportUnitInventoryExcel,
    saveFile
  } from '../lib/tauri';
  import { showSuccess, showError, showWarning } from '../lib/notifications';
  import { formatErrorMessage } from '../lib/errors';
  import type {
    Unit, Settings, UnitInventoryView,
    UnitMonthlySnapshot, ComputeSnapshotResult
  } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation } from '../lib/operationGuard';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const MONTHS_AR = [
    '', 'جانفي', 'فيفري', 'مارس', 'أفريل', 'ماي', 'جوان',
    'جويلية', 'أوت', 'سبتمبر', 'أكتوبر', 'نوفمبر', 'ديسمبر'
  ];

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());
  const unitsOp = createOperation({ scope });
  const monthsOp = createOperation({ scope });
  const computeOp = createOperation({ scope });
  const loadingUnits = unitsOp.loading;
  const loadingMonths = monthsOp.loading;
  const computing = computeOp.loading;

  // ── State ──────────────────────────────────────
  let units: Unit[] = $state([]);
  let availableMonths: [number, number][] = $state([]);
  let inventoryView: UnitInventoryView | null = $state(null);

  let selectedUnitId = $state('');
  let selectedYear = $state(new Date().getFullYear());
  let selectedMonth = $state(new Date().getMonth() + 1);
  let searchProduct = $state('');

  // ── Derived ────────────────────────────────────
  let filteredItems: UnitMonthlySnapshot[] = $derived(
    ((inventoryView as UnitInventoryView | null)?.items ?? []).filter((i: UnitMonthlySnapshot) =>
      i.product_name.includes(searchProduct) || !searchProduct
    )
  );

  let selectedUnitName: string = $derived(
    units.find(u => u.id === selectedUnitId)?.name ?? ''
  );

  let hasStaleData: boolean = $derived(
    ((inventoryView as UnitInventoryView | null)?.stale_count ?? 0) > 0
  );

  // ── Lifecycle ──────────────────────────────────
  onMount(async () => {
    await unitsOp.run(async () => {
      const settings = await getSettings();
      units = await listUnits(settings?.wilaya_code ?? '');
    });
  });

  // ── Event Handlers ─────────────────────────────
  async function onUnitChange() {
    inventoryView = null;
    availableMonths = [];
    if (!selectedUnitId) return;

    await monthsOp.run(async () => {
      availableMonths = await getAvailableReportMonths(selectedUnitId);
      if (availableMonths.length > 0) {
        [selectedYear, selectedMonth] = availableMonths[0];
      }
    });
  }

  async function loadInventory(forceRecompute = false) {
    if (!selectedUnitId) { showWarning('اختر وحدة أولاً'); return; }
    await computeOp.run(async () => {
      const result: ComputeSnapshotResult = await computeUnitInventorySnapshot(
        selectedUnitId, selectedYear, selectedMonth, forceRecompute
      );

      if (result.products_computed === 0 && !result.already_existed) {
        showWarning(`لا توجد بيانات للوحدة في ${MONTHS_AR[selectedMonth]} ${selectedYear}`);
        inventoryView = null;
        return;
      }

      inventoryView = await getUnitInventoryView(
        selectedUnitId, selectedYear, selectedMonth
      );

      if (result.balance_anomalies > 0 || result.consumption_anomalies > 0) {
        showWarning(
          `${result.balance_anomalies} شذوذ رصيد، ${result.consumption_anomalies} شذوذ استهلاك`
        );
      } else if (!result.already_existed) {
        showSuccess(`تم حساب ${result.products_computed} منتج بنجاح`);
      }
    });
  }

  async function handleExport() {
    if (!inventoryView) return;
    try {
      const path = await saveFile({
        filters: [{ name: 'Excel', extensions: ['xlsx'] }],
        defaultPath: `مخزون_${selectedUnitName}_${MONTHS_AR[selectedMonth]}_${selectedYear}.xlsx`
      });
      if (path) {
        const n = await exportUnitInventoryExcel(
          selectedUnitId, selectedYear, selectedMonth, path as string
        );
        showSuccess(`تم تصدير ${n} منتج`);
      }
    } catch (e) {
      showError('خطأ في التصدير');
    }
  }

</script>

<Layout nodeType="WILAYA" title="مخزون الوحدات"
        subtitle="عرض المخزون الشهري لكل وحدة">

  <div dir="rtl">
    <!-- ══ Section 1: اختيار الوحدة والشهر ══ -->
    <AppCard class="mb-6">
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4 items-end">
        <!-- الوحدة -->
        <div>
          {#if $loadingUnits}
            <div class="animate-pulse h-10 bg-gray-200 dark:bg-gray-700 rounded-lg"></div>
          {:else}
            <AppSelect
              id="unit-select"
              label="الوحدة *"
              bind:value={selectedUnitId}
              on:change={onUnitChange}
            >
              <option value="">-- اختر وحدة --</option>
              {#each units as u}
                <option value={u.id}>{u.name} ({u.code})</option>
              {/each}
            </AppSelect>
          {/if}
        </div>

        <!-- الشهر -->
        <div>
          {#if $loadingMonths}
            <div class="animate-pulse h-10 bg-gray-200 dark:bg-gray-700 rounded-lg"></div>
          {:else if availableMonths.length > 0}
            <AppSelect
              id="month-select"
              label="الشهر"
              on:change={(e: Event) => {
                const [y,m] = (e.target as HTMLSelectElement).value.split('-');
                selectedYear = +y; selectedMonth = +m;
              }}
              value="{selectedYear}-{selectedMonth}"
            >
              {#each availableMonths as [y, m]}
                <option value="{y}-{m}">{MONTHS_AR[m]} {y}</option>
              {/each}
            </AppSelect>
          {:else}
            <div>
              <span class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">الشهر</span>
              <div class="px-3 py-2 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-lg text-sm text-gray-400">
                {selectedUnitId ? 'لا توجد تقارير مستوردة' : 'اختر وحدة أولاً'}
              </div>
            </div>
          {/if}
        </div>

        <!-- زر العرض -->
        <div>
          <span class="block text-sm font-medium text-transparent mb-1 hidden md:block" aria-hidden="true">عرض</span>
          <AppButton
            variant="primary"
            fullWidth
            loading={$computing}
            disabled={!selectedUnitId || $computing || !availableMonths.length}
            on:click={() => loadInventory(false)}
          >
            عرض المخزون
          </AppButton>
        </div>

        <!-- أزرار إضافية -->
        <div class="flex gap-2 h-[42px]">
          {#if inventoryView}
            <div class="flex-1">
              <AppButton variant="secondary" fullWidth disabled={$computing} on:click={() => loadInventory(true)} title="إعادة الحساب">
                ↺ تحديث
              </AppButton>
            </div>
            <div class="flex-1">
              <AppButton variant="secondary" fullWidth on:click={handleExport}>
                Excel ↓
              </AppButton>
            </div>
          {/if}
        </div>
      </div>
    </AppCard>

    {#if inventoryView}

      <!-- ══ تنبيه: بيانات قديمة ══ -->
      {#if hasStaleData}
        <div class="mb-4">
          <AppAlert intent="warning" title="تنبيه">
            البيانات قديمة — وُجدت {inventoryView.stale_count} حركة جديدة منذ آخر حساب.
            <button onclick={() => loadInventory(true)} class="underline font-medium mr-1 hover:text-yellow-900 dark:hover:text-yellow-200">اضغط لإعادة الحساب</button>
          </AppAlert>
        </div>
      {/if}

      <!-- ══ Section 2: KPI Cards ══ -->
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        <AppCard class="border-t-4 border-blue-500" padding="sm">
          <p class="text-xs text-gray-500 dark:text-gray-400">إجمالي المنتجات</p>
          <p class="text-2xl font-bold text-blue-600 dark:text-blue-400 mt-1">{inventoryView.total_products}</p>
        </AppCard>

        <AppCard class="border-t-4 {inventoryView.balance_anomaly_count > 0 ? 'border-red-500' : 'border-green-500'}" padding="sm">
          <p class="text-xs text-gray-500 dark:text-gray-400">شذوذات الرصيد</p>
          <p class="text-2xl font-bold mt-1 {inventoryView.balance_anomaly_count > 0 ? 'text-red-600 dark:text-red-400' : 'text-green-600 dark:text-green-400'}">
            {inventoryView.balance_anomaly_count}
          </p>
        </AppCard>

        <!-- شذوذات الاستهلاك -->
        <AppCard class="border-t-4 {inventoryView.consumption_anomaly_count > 0 ? 'border-orange-500' : 'border-green-500'}" padding="sm">
          <p class="text-xs text-gray-500 dark:text-gray-400">استهلاك شاذ</p>
          <p class="text-2xl font-bold mt-1 {inventoryView.consumption_anomaly_count > 0 ? 'text-orange-600 dark:text-orange-400' : 'text-green-600 dark:text-green-400'}">
            {inventoryView.consumption_anomaly_count}
          </p>
          <p class="text-xs text-gray-400 dark:text-gray-500 mt-1">> 1.5× المتوسط</p>
        </AppCard>

        <AppCard class="border-t-4 border-gray-300 dark:border-gray-700" padding="sm">
          <p class="text-xs text-gray-500 dark:text-gray-400">آخر حساب</p>
          <p class="text-xs font-medium text-gray-600 dark:text-gray-300 mt-2">
            {new Date(inventoryView.computed_at).toLocaleString('ar-DZ')}
          </p>
        </AppCard>
      </div>

      <!-- تنبيه الشذوذات -->
      {#if inventoryView.has_any_anomaly}
        <div class="mb-6">
          <AppAlert intent="danger" title="⚠️ تم اكتشاف شذوذات:">
            <ul class="text-sm mt-1 space-y-1 list-disc list-inside">
              {#if inventoryView.balance_anomaly_count > 0}
                <li>
                  {inventoryView.balance_anomaly_count} منتج: الرصيد المُبلَّغ ≠ (أولي + دخول − خروج)
                </li>
              {/if}
              {#if inventoryView.consumption_anomaly_count > 0}
                <li>
                  {inventoryView.consumption_anomaly_count} منتج: الاستهلاك يتجاوز 1.5× المتوسط الثلاثي
                </li>
              {/if}
            </ul>
          </AppAlert>
        </div>
      {/if}

      <!-- ══ Section 3: الجدول الكامل ══ -->
      <AppCard padding="none">
        <div class="p-4 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between bg-white dark:bg-gray-800 rounded-t-xl">
          <h3 class="font-semibold text-lg text-gray-800 dark:text-gray-100">
            {inventoryView.unit_name} — {MONTHS_AR[inventoryView.report_month]} {inventoryView.report_year}
          </h3>
          <input type="text" bind:value={searchProduct}
                 placeholder="بحث عن منتج..."
                 class="px-3 py-1.5 border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 rounded-lg text-sm w-48 transition-colors" />
        </div>

        <AppTable empty={filteredItems.length === 0}>
          <svelte:fragment slot="empty">
            <AppEmptyState title="لا توجد نتائج" description="لم يتم العثور على منتجات تطابق معايير البحث." icon="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svelte:fragment>

          <svelte:fragment slot="head">
            <th class="table-header">المنتج</th>
            <th class="table-header text-center">المخزون الأولي</th>
            <th class="table-header text-center text-green-700 dark:text-green-500">دخول (+)</th>
            <th class="table-header text-center text-red-700 dark:text-red-500">خروج (−)</th>
            <th class="table-header text-center">المتبقي النظري</th>
            <th class="table-header text-center">المتبقي الفعلي</th>
            <th class="table-header text-center">الفارق</th>
            <th class="table-header text-center">الحالة</th>
          </svelte:fragment>

          {#each filteredItems as item}
            <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors border-b dark:border-gray-700
              {item.has_balance_anomaly ? 'bg-red-50/50 dark:bg-red-900/20 border-r-4 border-r-red-400' :
               item.has_consumption_anomaly ? 'bg-orange-50/50 dark:bg-orange-900/20 border-r-4 border-r-orange-400' : ''}">

              <!-- المنتج -->
              <td class="table-cell font-medium">
                <div class="flex items-center gap-1">
                  {#if item.has_balance_anomaly}
                    <span title="شذوذ رصيد" class="text-red-500">⚠️</span>
                  {:else if item.has_consumption_anomaly}
                    <span title="استهلاك شاذ" class="text-orange-500">📊</span>
                  {:else}
                    <span class="text-green-500">✓</span>
                  {/if}
                  {item.product_name}
                  {#if item.is_stale}
                    <span title="بيانات قديمة" class="text-yellow-500 text-xs">⏱</span>
                  {/if}
                </div>
                {#if item.has_consumption_anomaly && item.avg_consumption_3months}
                  <p class="text-xs text-orange-600 dark:text-orange-400 mt-0.5">
                    متوسط 3 أشهر: {item.avg_consumption_3months.toFixed(1)}
                  </p>
                {/if}
              </td>

              <td class="table-cell text-center">
                {item.opening_stock.toFixed(2)}
                {#if item.opening_stock === 0 && item.total_in > 0}
                  <span class="text-xs text-blue-500 dark:text-blue-400 mr-1">
                    (جديد)
                  </span>
                {/if}
              </td>

              <td class="table-cell text-center text-green-700 dark:text-green-500 font-medium">
                {item.total_in > 0 ? '+' + item.total_in.toFixed(2) : '—'}
              </td>

              <td class="table-cell text-center text-red-600 dark:text-red-500 font-medium">
                {item.total_out > 0 ? item.total_out.toFixed(2) : '—'}
              </td>

              <td class="table-cell text-center text-gray-500 dark:text-gray-400">
                {item.computed_closing.toFixed(2)}
              </td>

              <td class="table-cell text-center font-medium {item.reported_closing < 10 ? 'text-red-700 dark:text-red-500' : 'text-gray-800 dark:text-gray-200'}">
                {item.reported_closing.toFixed(2)}
              </td>

              <!-- الفارق -->
              <td class="table-cell text-center {item.has_balance_anomaly ? 'text-red-700 dark:text-red-500 font-bold' : 'text-gray-400 dark:text-gray-500'}">
                {Math.abs(item.variance) > 0.01
                  ? (item.variance > 0 ? '+' : '') + item.variance.toFixed(2)
                  : '—'}
              </td>

              <!-- الحالة -->
              <td class="table-cell text-center">
                {#if item.has_balance_anomaly}
                  <AppBadge intent="danger" size="sm">⚠️ فارق رصيد</AppBadge>
                {:else if item.has_consumption_anomaly}
                  <AppBadge intent="warning" size="sm">📊 استهلاك شاذ</AppBadge>
                {:else}
                  <AppBadge intent="success" size="sm">✅ سليم</AppBadge>
                {/if}
              </td>
            </tr>
          {/each}
        </AppTable>

        {#if filteredItems.length > 0}
          <div class="p-3 border-t border-gray-200 dark:border-gray-700 text-xs text-gray-500 dark:text-gray-400 flex justify-between rounded-b-xl bg-white dark:bg-gray-800">
            <span>{filteredItems.length} منتج</span>
            <span>
              المعادلة: المتبقي = الأولي + الدخول − الخروج
            </span>
          </div>
        {/if}
      </AppCard>

    {:else if selectedUnitId && !$computing}
      <AppCard class="p-12 text-center">
        <AppEmptyState
          title="اضغط 'عرض المخزون' لبدء الحساب"
          description="سيُحسب من حركات المخزون المستوردة"
          icon="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
        />
      </AppCard>
    {:else if !selectedUnitId}
      <AppCard class="p-12 text-center">
        <AppEmptyState
          title="لا توجد بيانات للعرض"
          description="اختر وحدة لعرض مخزونها الشهري"
          icon="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 002-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
        />
      </AppCard>
    {/if}
  </div>
</Layout>
