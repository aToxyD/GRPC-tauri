<script lang="ts">
  import { onMount } from 'svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import {
    listUnits, getSettings,
    computeUnitInventorySnapshot, getUnitInventoryView,
    getAvailableReportMonths, exportUnitInventoryExcel
  } from '../lib/tauri';
  import { showSuccess, showError, showWarning } from '../lib/notifications';
  import type {
    Unit, Settings, UnitInventoryView,
    UnitMonthlySnapshot, ComputeSnapshotResult
  } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  const MONTHS_AR = [
    '', 'جانفي', 'فيفري', 'مارس', 'أفريل', 'ماي', 'جوان',
    'جويلية', 'أوت', 'سبتمبر', 'أكتوبر', 'نوفمبر', 'ديسمبر'
  ];

  // ── State ──────────────────────────────────────
  let units: Unit[] = $state([]);
  let availableMonths: [number, number][] = $state([]);
  let inventoryView: UnitInventoryView | null = $state(null);

  let selectedUnitId = $state('');
  let selectedYear = $state(new Date().getFullYear());
  let selectedMonth = $state(new Date().getMonth() + 1);
  let searchProduct = $state('');

  let loadingUnits = $state(true);
  let loadingMonths = $state(false);
  let computing = $state(false);

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
    try {
      const settings = await getSettings();
      units = await listUnits(settings?.wilaya_code ?? '');
    } catch (e) {
      showError('خطأ في تحميل الوحدات');
    } finally {
      loadingUnits = false;
    }
  });

  // ── Event Handlers ─────────────────────────────
  async function onUnitChange() {
    inventoryView = null;
    availableMonths = [];
    if (!selectedUnitId) return;

    try {
      loadingMonths = true;
      availableMonths = await getAvailableReportMonths(selectedUnitId);
      if (availableMonths.length > 0) {
        [selectedYear, selectedMonth] = availableMonths[0];
      }
    } catch (e) {
      showError('خطأ في جلب الأشهر');
    } finally {
      loadingMonths = false;
    }
  }

  async function loadInventory(forceRecompute = false) {
    if (!selectedUnitId) { showWarning('اختر وحدة أولاً'); return; }
    try {
      computing = true;
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

      // إشعارات الشذوذات
      if (result.balance_anomalies > 0 || result.consumption_anomalies > 0) {
        showWarning(
          `${result.balance_anomalies} شذوذ رصيد، ${result.consumption_anomalies} شذوذ استهلاك`
        );
      } else if (!result.already_existed) {
        showSuccess(`تم حساب ${result.products_computed} منتج بنجاح`);
      }
    } catch (e) {
      showError('خطأ في الحساب: ' + String(e));
    } finally {
      computing = false;
    }
  }

  async function handleExport() {
    if (!inventoryView) return;
    try {
      const path = await save({
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

  <!-- ══ Section 1: اختيار الوحدة والشهر ══ -->
  <div class="card mb-6 p-4">
    <div class="grid grid-cols-1 md:grid-cols-4 gap-4 items-end">

      <!-- الوحدة -->
      <div>
        <label for="unit-select" class="block text-sm font-medium text-gray-700 mb-1">الوحدة *</label>
        {#if loadingUnits}
          <div class="animate-pulse h-10 bg-gray-200 rounded-lg"></div>
        {:else}
          <select id="unit-select" bind:value={selectedUnitId} onchange={onUnitChange}
                  class="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm">
            <option value="">-- اختر وحدة --</option>
            {#each units as u}
              <option value={u.id}>{u.name} ({u.code})</option>
            {/each}
          </select>
        {/if}
      </div>

      <!-- الشهر -->
      <div>
        <label for="month-select" class="block text-sm font-medium text-gray-700 mb-1">الشهر</label>
        {#if loadingMonths}
          <div class="animate-pulse h-10 bg-gray-200 rounded-lg"></div>
        {:else if availableMonths.length > 0}
          <select id="month-select" class="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
                  onchange={(e) => {
                    const [y,m] = (e.target as HTMLSelectElement).value.split('-');
                    selectedYear = +y; selectedMonth = +m;
                  }}
                  value="{selectedYear}-{selectedMonth}">
            {#each availableMonths as [y, m]}
              <option value="{y}-{m}">{MONTHS_AR[m]} {y}</option>
            {/each}
          </select>
        {:else}
          <div class="px-3 py-2 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-400">
            {selectedUnitId ? 'لا توجد تقارير مستوردة' : 'اختر وحدة أولاً'}
          </div>
        {/if}
      </div>

      <!-- زر العرض -->
      <button onclick={() => loadInventory(false)}
              disabled={!selectedUnitId || computing || !availableMonths.length}
              class="btn-primary flex items-center justify-center gap-2
                     disabled:opacity-50 disabled:cursor-not-allowed h-10">
        {#if computing}
          <div class="animate-spin h-4 w-4 border-2 border-white border-t-transparent rounded-full"></div>
          جارٍ الحساب...
        {:else}
          عرض المخزون
        {/if}
      </button>

      <!-- أزرار إضافية -->
      <div class="flex gap-2">
        {#if inventoryView}
          <button onclick={() => loadInventory(true)} disabled={computing}
                  class="btn-secondary text-sm flex-1" title="إعادة الحساب">
            ↺ تحديث
          </button>
          <button onclick={handleExport} class="btn-secondary text-sm flex-1">
            Excel ↓
          </button>
        {/if}
      </div>

    </div>
  </div>

  {#if inventoryView}

    <!-- ══ تنبيه: بيانات قديمة ══ -->
    {#if hasStaleData}
      <div class="mb-4 p-3 bg-yellow-50 border border-yellow-300 rounded-lg
                  flex items-center gap-2 text-sm text-yellow-800">
        <span class="text-lg">⚠️</span>
        <span>
          البيانات قديمة — وُجدت {inventoryView.stale_count} حركة جديدة منذ آخر حساب.
          <button onclick={() => loadInventory(true)}
                  class="underline font-medium mr-1">اضغط لإعادة الحساب</button>
        </span>
      </div>
    {/if}

    <!-- ══ Section 2: KPI Cards ══ -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">

      <div class="card p-4 border-l-4 border-blue-500">
        <p class="text-xs text-gray-500">إجمالي المنتجات</p>
        <p class="text-2xl font-bold text-blue-600">{inventoryView.total_products}</p>
      </div>

      <div class="card p-4 border-l-4
                  {inventoryView.balance_anomaly_count > 0 ? 'border-red-500' : 'border-green-500'}">
        <p class="text-xs text-gray-500">شذوذات الرصيد</p>
        <p class="text-2xl font-bold
                  {inventoryView.balance_anomaly_count > 0 ? 'text-red-600' : 'text-green-600'}">
          {inventoryView.balance_anomaly_count}
        </p>
      </div>

      <!-- شذوذات الاستهلاك -->
      <div class="card p-4 border-l-4
                  {inventoryView.consumption_anomaly_count > 0 ? 'border-orange-500' : 'border-green-500'}">
        <p class="text-xs text-gray-500">استهلاك شاذ</p>
        <p class="text-2xl font-bold
                  {inventoryView.consumption_anomaly_count > 0 ? 'text-orange-600' : 'text-green-600'}">
          {inventoryView.consumption_anomaly_count}
        </p>
        <p class="text-xs text-gray-400 mt-1">> 1.5× المتوسط</p>
      </div>

      <div class="card p-4 border-l-4 border-gray-300">
        <p class="text-xs text-gray-500">آخر حساب</p>
        <p class="text-xs font-medium text-gray-600 mt-1">
          {new Date(inventoryView.computed_at).toLocaleString('ar-DZ')}
        </p>
      </div>

    </div>

    <!-- تنبيه الشذوذات -->
    {#if inventoryView.has_any_anomaly}
      <div class="mb-6 p-4 bg-red-50 border border-red-200 rounded-lg">
        <p class="font-semibold text-red-700">⚠️ تم اكتشاف شذوذات:</p>
        <ul class="text-sm text-red-600 mt-1 space-y-1 list-disc list-inside">
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
      </div>
    {/if}

    <!-- ══ Section 3: الجدول الكامل ══ -->
    <div class="card">
      <div class="p-4 border-b flex items-center justify-between">
        <h3 class="font-semibold text-lg">
          {inventoryView.unit_name} — {MONTHS_AR[inventoryView.report_month]} {inventoryView.report_year}
        </h3>
        <input type="text" bind:value={searchProduct}
               placeholder="بحث عن منتج..."
               class="px-3 py-1.5 border rounded-lg text-sm w-48" />
      </div>

      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="bg-gray-50 text-xs text-gray-500 uppercase">
              <th class="table-header">المنتج</th>
              <th class="table-header text-center">المخزون الأولي</th>
              <th class="table-header text-center text-green-700">دخول (+)</th>
              <th class="table-header text-center text-red-700">خروج (−)</th>
              <th class="table-header text-center">المتبقي النظري</th>
              <th class="table-header text-center">المتبقي الفعلي</th>
              <th class="table-header text-center">الفارق</th>
              <th class="table-header text-center">الحالة</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredItems as item}
              <tr class="hover:bg-gray-50 border-b
                {item.has_balance_anomaly ? 'bg-red-50 border-r-4 border-red-400' :
                 item.has_consumption_anomaly ? 'bg-orange-50 border-r-4 border-orange-400' : ''}">

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
                    <p class="text-xs text-orange-600 mt-0.5">
                      متوسط 3 أشهر: {item.avg_consumption_3months.toFixed(1)}
                    </p>
                  {/if}
                </td>

                <td class="table-cell text-center">
                  {item.opening_stock.toFixed(2)}

                  {#if item.opening_stock === 0 && item.total_in > 0}
                    <span class="text-xs text-blue-500 mr-1">
                      (جديد)
                    </span>
                  {/if}

                </td>

                <td class="table-cell text-center text-green-700 font-medium">
                  {item.total_in > 0 ? '+' + item.total_in.toFixed(2) : '—'}
                </td>

                <td class="table-cell text-center text-red-600 font-medium">
                  {item.total_out > 0 ? item.total_out.toFixed(2) : '—'}
                </td>

                <td class="table-cell text-center text-gray-500">
                  {item.computed_closing.toFixed(2)}
                </td>

                <td class="table-cell text-center font-medium
                  {item.reported_closing < 10 ? 'text-red-700' : 'text-gray-800'}">
                  {item.reported_closing.toFixed(2)}
                </td>

                <!-- الفارق -->
                <td class="table-cell text-center
                  {item.has_balance_anomaly ? 'text-red-700 font-bold' : 'text-gray-400'}">
                  {Math.abs(item.variance) > 0.01
                    ? (item.variance > 0 ? '+' : '') + item.variance.toFixed(2)
                    : '—'}
                </td>

                <!-- الحالة -->
                <td class="table-cell text-center">
                  {#if item.has_balance_anomaly}
                    <span class="px-2 py-0.5 rounded-full text-xs bg-red-100 text-red-700">
                      ⚠️ فارق رصيد
                    </span>
                  {:else if item.has_consumption_anomaly}
                    <span class="px-2 py-0.5 rounded-full text-xs bg-orange-100 text-orange-700">
                      📊 استهلاك شاذ
                    </span>
                  {:else}
                    <span class="px-2 py-0.5 rounded-full text-xs bg-green-100 text-green-700">
                      ✅ سليم
                    </span>
                  {/if}
                </td>

              </tr>
            {/each}

            {#if filteredItems.length === 0}
              <tr>
                <td colspan="8" class="py-8 text-center text-gray-400">
                  لا توجد نتائج
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>

      {#if filteredItems.length > 0}
        <div class="p-3 border-t text-xs text-gray-500 flex justify-between">
          <span>{filteredItems.length} منتج</span>
          <span>
            المعادلة: المتبقي = الأولي + الدخول − الخروج
          </span>
        </div>
      {/if}
    </div>

  {:else if selectedUnitId && !computing}
    <div class="card p-12 text-center text-gray-400">
      <p class="text-lg mb-2">اضغط "عرض المخزون" لبدء الحساب</p>
      <p class="text-sm">سيُحسب من حركات المخزون المستوردة</p>
    </div>
  {:else if !selectedUnitId}
    <div class="card p-12 text-center text-gray-400">
      <p class="text-lg">اختر وحدة لعرض مخزونها الشهري</p>
    </div>
  {/if}

</Layout>
