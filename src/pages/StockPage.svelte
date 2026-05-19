<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { formatErrorMessage } from "../lib/errors";
  import {
    getAllStocks,
    importProductsPackage,
    getStockSummary,
    getStockMovements,
    exportStockMovementsExcel,
    exportStockMovementsPackage,
    importStockMovementsPackage,
    getSettings,
    openFile,
    saveFile,
  } from "../lib/tauri";
  import { showSuccess, showError } from "../lib/notifications";
  import type {
    InventoryStock,
    StockSummary,
    StockMovement,
    StockMovementFilters,
    StockMovementResponse,
    StockMovementType,
    Settings,
  } from "../lib/types";
  import Layout from "../components/Layout.svelte";

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  // Section 2 & 3: Summary
  let summary: StockSummary[] = $state([]);
  let summaryLoading = $state(true);

  // Computed stats from summary
  let totalProducts = $derived(summary.length);
  let totalIn = $derived(summary.filter((p) => p.total_in > 0).length);
  let totalOut = $derived(summary.filter((p) => p.total_out > 0).length);
  let lowStockCount = $derived(
    summary.filter((p) => p.current_quantity < 10).length,
  );

  // Section 4: Movements
  let movements: StockMovement[] = $state([]);
  let movementsLoading = $state(false);
  let movementsVisible = $state(false);
  let totalMovements = $state(0);
  let currentPage = $state(0);
  let highlightedProductId = $state("");
  const PAGE_SIZE = 20;

  // Filters
  let filterProductId = $state("");
  let filterMovementType = $state("");
  let filterStartDate = $state("");
  let filterEndDate = $state("");

  // Messages (for import only)
  let importError = $state("");
  let importSuccess = $state("");
  let settings: Settings | null = $state(null);

  let stockTimeouts: number[] = [];
  onDestroy(() => {
    stockTimeouts.forEach(clearTimeout);
  });

  // Fallback stocks
  let stocks: InventoryStock[] = $state([]);

  onMount(async () => {
    try {
      settings = await getSettings();
    } catch (e) {
      console.error("Failed to load settings", e);
    }
    await loadSummary();
  });

  async function loadSummary() {
    try {
      summaryLoading = true;
      summary = await getStockSummary();
    } catch (e) {
      showError("خطأ في تحميل ملخص المخزون: " + formatErrorMessage(e));
      // Fallback to old method
      try {
        stocks = await getAllStocks();
      } catch (e2) {
        showError("خطأ في التحميل الاحتياطي: " + formatErrorMessage(e2));
      }
    } finally {
      summaryLoading = false;
    }
  }

  async function loadMovements() {
    try {
      movementsLoading = true;
      movementsVisible = true;

      const filters: StockMovementFilters = {};
      if (filterProductId) filters.product_id = filterProductId;
      if (filterMovementType)
        filters.movement_type = filterMovementType as StockMovementType;
      if (filterStartDate) filters.start_date = filterStartDate;
      if (filterEndDate) filters.end_date = filterEndDate;

      const response: StockMovementResponse = await getStockMovements(
        filters,
        currentPage,
        PAGE_SIZE,
      );
      movements = response.movements;
      totalMovements = response.total_count;
    } catch (e) {
      showError("خطأ في تحميل حركات المخزون: " + formatErrorMessage(e));
    } finally {
      movementsLoading = false;
    }
  }

  async function showMovementsForProduct(productId: string) {
    filterProductId = productId;
    highlightedProductId = productId;
    currentPage = 0;
    await loadMovements(); // انتظر حتى يظهر القسم
    // ثم scroll
    const t = window.setTimeout(() => {
      document
        .getElementById("movements-section")
        ?.scrollIntoView({ behavior: "smooth" });
    }, 100);
    stockTimeouts.push(t);
  }

  function clearFilters() {
    filterProductId = "";
    filterMovementType = "";
    filterStartDate = "";
    filterEndDate = "";
    highlightedProductId = "";
    currentPage = 0;
    loadMovements();
  }

  async function handleExportMovements() {
    try {
      const filePath = await saveFile({
        filters: [{ name: "Excel", extensions: ["xlsx"] }],
      });

      if (filePath) {
        const count = await exportStockMovementsExcel(
          filterProductId || undefined,
          filePath as string,
        );
        showSuccess(`تم تصدير ${count} حركة إلى Excel بنجاح`);
      }
    } catch (e) {
      showError("خطأ في التصدير: " + formatErrorMessage(e));
    }
  }

  async function handleImportProducts() {
    try {
      importError = "";
      importSuccess = "";

      const selected = await openFile({
        multiple: false,
        filters: [
          {
            name: "حزمة المزامنة",
            extensions: ["sync"],
          },
        ],
      });

      if (selected) {
        const result = await importProductsPackage(selected as string);
        const count = result.added;
        importSuccess = `تم استيراد ${count} منتجات بنجاح`;
        await loadSummary();
        const t = window.setTimeout(() => (importSuccess = ""), 3000);
        stockTimeouts.push(t);
      }
    } catch (e) {
      importError = "خطأ في الاستيراد: " + formatErrorMessage(e);
    }
  }


  async function handleExportMovementsPackage() {
    try {
      const now = new Date();
      const dateStr = now.toISOString().split("T")[0];
      const unitCode = settings?.unit_code || "UNIT";
      const defaultFilename = `حزمة_حركة_المخزون_${unitCode}_${dateStr}.sync`;

      const filePath = await saveFile({
        defaultPath: defaultFilename,
        filters: [{ name: "Sync Package", extensions: ["sync"] }],
      });

      if (filePath) {
        const startOfMonth = new Date(now.getFullYear(), now.getMonth(), 1);
        const startDate =
          startOfMonth.toISOString().split("T")[0] + "T00:00:00Z";
        const endDate = now.toISOString().split("T")[0] + "T23:59:59Z";

        const result = await exportStockMovementsPackage(
          startDate,
          endDate,
          filePath as string,
        );
        showSuccess(
          `تم تصدير ${result.record_count} حركة مخزون إلى حزمة مشفرة بنجاح`,
        );
      }
    } catch (e) {
      showError("خطأ في تصدير حزمة حركات المخزون: " + formatErrorMessage(e));
    }
  }

  async function nextPage() {
    currentPage++;
    await loadMovements();
  }

  async function prevPage() {
    currentPage--;
    await loadMovements();
  }

  function getStatusBadge(quantity: number): { text: string; intent: 'success' | 'warning' | 'danger' | 'neutral' } {
    if (quantity === 0) {
      return { text: "نفد", intent: "neutral" };
    } else if (quantity < 10) {
      return { text: "منخفض", intent: "danger" };
    } else if (quantity < 50) {
      return { text: "متوسط", intent: "warning" };
    } else {
      return { text: "جيد", intent: "success" };
    }
  }

  function getMovementTypeBadge(type: StockMovementType): { text: string; intent: 'success' | 'warning' | 'danger' | 'neutral' | 'info' } {
    switch (type) {
      case "IN":
        return { text: "↑ دخول", intent: "success" };
      case "OUT":
        return { text: "↓ خروج", intent: "danger" };
      case "OPENING":
        return { text: "◉ افتتاحي", intent: "neutral" };
      default:
        return { text: type, intent: "neutral" };
    }
  }

  function getMovementQuantityColor(type: StockMovementType) {
    return type === "IN" || type === "OPENING"
      ? "text-green-600 dark:text-green-400"
      : "text-red-600 dark:text-red-400";
  }

  function formatDate(dateStr: string) {
    return new Date(dateStr).toLocaleString("ar-DZ");
  }

  let totalPages = $derived(Math.ceil(totalMovements / PAGE_SIZE));
</script>

<Layout
  nodeType="UNIT"
  title="حالة المخزون"
  subtitle="ملخص المخزون وسجل الحركات"
>
  <div dir="rtl">
    <!-- SECTION 1: Header -->
    <div class="mb-8">
      <AppPageHeader title="حالة المخزون" subtitle="ملخص المخزون وسجل الحركات">
        <svelte:fragment slot="actions">
          <div class="flex items-center gap-2 flex-wrap">
            <AppButton variant="secondary" on:click={handleExportMovements}>
              <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
              </svg>
              تصدير الحركات (Excel)
            </AppButton>
            
            <AppButton variant="secondary" on:click={handleExportMovementsPackage} ariaLabel="تصدير حركات المخزون كحزمة مشفرة وآمنة للمزامنة مع الولاية">
              <svg class="w-4 h-4 mr-2 inline-block text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/>
              </svg>
              <span class="text-blue-700 dark:text-blue-400">تصدير حزمة حركات (.sync)</span>
            </AppButton>

            <AppButton variant="secondary" on:click={handleImportProducts} ariaLabel="استيراد حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد">
              <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
              </svg>
              استيراد منتجات الولاية
            </AppButton>
          </div>
        </svelte:fragment>
      </AppPageHeader>
    </div>

    {#if importError}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => importError = ''}>{importError}</AppAlert>
      </div>
    {/if}

    {#if importSuccess}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => importSuccess = ''}>{importSuccess}</AppAlert>
      </div>
    {/if}

    <!-- SECTION 2: Summary Cards -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
      <AppCard class="border-r-4 border-blue-500" padding="sm">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400">إجمالي المنتجات</p>
            <p class="text-2xl font-bold text-blue-600 dark:text-blue-400">
              {totalProducts.toLocaleString("ar-DZ")}
            </p>
          </div>
          <div class="w-12 h-12 bg-blue-100 dark:bg-blue-900/50 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-blue-600 dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/>
            </svg>
          </div>
        </div>
      </AppCard>

      <AppCard class="border-r-4 border-green-500" padding="sm">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400">إجمالي الدخول</p>
            <p class="text-2xl font-bold text-green-600 dark:text-green-400">
              {totalIn.toLocaleString("ar-DZ")}
            </p>
          </div>
          <div class="w-12 h-12 bg-green-100 dark:bg-green-900/50 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 10l7-7m0 0l7 7m-7-7v18"/>
            </svg>
          </div>
        </div>
      </AppCard>

      <AppCard class="border-r-4 border-red-500" padding="sm">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400">إجمالي الخروج</p>
            <p class="text-2xl font-bold text-red-600 dark:text-red-400">
              {totalOut.toLocaleString("ar-DZ")}
            </p>
          </div>
          <div class="w-12 h-12 bg-red-100 dark:bg-red-900/50 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-red-600 dark:text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3"/>
            </svg>
          </div>
        </div>
      </AppCard>

      <AppCard class="border-r-4 border-orange-500" padding="sm">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400">منتجات منخفضة</p>
            <p class="text-2xl font-bold text-orange-600 dark:text-orange-400">
              {lowStockCount.toLocaleString("ar-DZ")}
            </p>
          </div>
          <div class="w-12 h-12 bg-orange-100 dark:bg-orange-900/50 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-orange-600 dark:text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
            </svg>
          </div>
        </div>
      </AppCard>
    </div>

    <!-- SECTION 3: Stock Table -->
    <div class="mb-8">
      <AppCard padding="none">
        <div class="p-4 border-b border-gray-200 dark:border-gray-700">
          <h3 class="font-semibold text-lg text-gray-800 dark:text-white">المخزون الحالي</h3>
        </div>

        <AppTable
          loading={summaryLoading}
          empty={!summaryLoading && summary.length === 0 && stocks.length === 0}
        >
          <svelte:fragment slot="empty">
            <AppEmptyState
              title="لا يوجد منتجات في المخزون"
              description="استورد قائمة منتجات الولاية"
              icon="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
            />
          </svelte:fragment>

          <svelte:fragment slot="head">
            <th class="table-header">المنتج</th>
            <th class="table-header">الكمية الحالية</th>
            <th class="table-header">إجمالي الدخول</th>
            <th class="table-header">إجمالي الخروج</th>
            <th class="table-header">عدد الحركات</th>
            <th class="table-header">آخر حركة</th>
            <th class="table-header">الحالة</th>
            <th class="table-header text-left">إجراءات</th>
          </svelte:fragment>

          {#each summary as product}
            {@const status = getStatusBadge(product.current_quantity)}
            {@const isHighlighted = highlightedProductId === product.product_id}
            <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors {isHighlighted ? 'bg-blue-50 dark:bg-blue-900/20' : ''}">
              <td class="table-cell font-medium">{product.product_name}</td>
              <td class="table-cell {product.current_quantity < 10 ? 'text-red-700 dark:text-red-400 font-bold' : ''}">
                {product.current_quantity.toFixed(2)}
              </td>
              <td class="table-cell text-green-600 dark:text-green-400">
                {product.total_in.toFixed(2)}
              </td>
              <td class="table-cell text-red-600 dark:text-red-400">
                {product.total_out.toFixed(2)}
              </td>
              <td class="table-cell">{product.movement_count}</td>
              <td class="table-cell text-sm text-gray-500 dark:text-gray-400">
                {product.last_movement ? formatDate(product.last_movement) : "-"}
              </td>
              <td class="table-cell">
                <AppBadge intent={status.intent} size="sm">{status.text}</AppBadge>
              </td>
              <td class="table-cell text-left">
                <AppButton variant="ghost" size="sm" class="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300" on:click={() => showMovementsForProduct(product.product_id)}>
                  الحركات
                </AppButton>
              </td>
            </tr>
          {/each}
        </AppTable>
      </AppCard>
    </div>

    <!-- SECTION 4: Movements Table -->
    {#if movementsVisible}
      <div id="movements-section" class="mb-8">
        <AppCard padding="none">
          <div class="p-4 border-b border-gray-200 dark:border-gray-700">
            <h3 class="font-semibold text-lg text-gray-800 dark:text-white">سجل حركات المخزون</h3>
          </div>

          <!-- Filters -->
          <div class="p-4 bg-gray-50 dark:bg-gray-900 border-b border-gray-200 dark:border-gray-700">
            <div class="grid grid-cols-1 md:grid-cols-4 gap-4 mb-4">
              <AppSelect
                id="filter-product"
                label="المنتج"
                bind:value={filterProductId}
              >
                <option value="">كل المنتجات</option>
                {#each summary as product}
                  <option value={product.product_id}>{product.product_name}</option>
                {/each}
              </AppSelect>

              <AppSelect
                id="filter-type"
                label="نوع الحركة"
                bind:value={filterMovementType}
              >
                <option value="">كل الأنواع</option>
                <option value="IN">دخول مخزون</option>
                <option value="OUT">خروج مخزون</option>
                <option value="OPENING">رصيد افتتاحي</option>
              </AppSelect>

              <AppInput
                id="filter-start"
                label="من تاريخ"
                type="date"
                bind:value={filterStartDate}
              />

              <AppInput
                id="filter-end"
                label="إلى تاريخ"
                type="date"
                bind:value={filterEndDate}
              />
            </div>

            <div class="flex gap-3">
              <AppButton variant="primary" on:click={loadMovements}>بحث</AppButton>
              <AppButton variant="secondary" on:click={clearFilters}>مسح</AppButton>
            </div>
          </div>

          <!-- Movements Table -->
          <AppTable
            loading={movementsLoading}
            empty={!movementsLoading && movements.length === 0}
          >
            <svelte:fragment slot="empty">
              <div class="text-center py-8 text-gray-500 dark:text-gray-400">
                <p>لا توجد حركات تطابق الفلاتر المحددة</p>
              </div>
            </svelte:fragment>

            <svelte:fragment slot="head">
              <th class="table-header">التاريخ والوقت</th>
              <th class="table-header">المنتج</th>
              <th class="table-header">نوع الحركة</th>
              <th class="table-header">الكمية</th>
              <th class="table-header">الرصيد قبل</th>
              <th class="table-header">الرصيد بعد</th>
              <th class="table-header">المرجع</th>
              <th class="table-header">المستخدم</th>
            </svelte:fragment>

            {#each movements as movement}
              {@const typeBadge = getMovementTypeBadge(movement.movement_type)}
              {@const qtyColor = getMovementQuantityColor(movement.movement_type)}
              {@const refShort = movement.reference_id ? movement.reference_id.slice(0, 8) : "-"}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors">
                <td class="table-cell text-sm">{formatDate(movement.timestamp)}</td>
                <td class="table-cell font-medium">{movement.product_name || "-"}</td>
                <td class="table-cell">
                  <AppBadge intent={typeBadge.intent} size="sm">{typeBadge.text}</AppBadge>
                </td>
                <td class="table-cell font-medium {qtyColor}">
                  {movement.movement_type === "OUT" ? "-" : "+"}{movement.quantity.toFixed(2)}
                </td>
                <td class="table-cell text-gray-500 dark:text-gray-400">{movement.balance_before.toFixed(2)}</td>
                <td class="table-cell text-gray-500 dark:text-gray-400">{movement.balance_after.toFixed(2)}</td>
                <td class="table-cell text-sm">
                  {#if movement.reference_type}
                    <span class="text-gray-600 dark:text-gray-400">{movement.reference_type}:</span>
                    <span class="font-mono text-xs text-gray-800 dark:text-gray-200">{refShort}</span>
                  {:else}
                    <span class="text-gray-400">-</span>
                  {/if}
                </td>
                <td class="table-cell text-sm">{movement.username}</td>
              </tr>
            {/each}
          </AppTable>

          <!-- Pagination -->
          {#if !movementsLoading && movements.length > 0}
            <div class="p-4 border-t border-gray-200 dark:border-gray-700 flex items-center justify-between">
              <p class="text-sm text-gray-600 dark:text-gray-400">
                الصفحة {currentPage + 1} من {totalPages || 1} | إجمالي: {totalMovements.toLocaleString("ar-DZ")} حركة
              </p>
              <div class="flex gap-2">
                <AppButton variant="secondary" size="sm" disabled={currentPage === 0} on:click={prevPage}>السابق</AppButton>
                <AppButton variant="secondary" size="sm" disabled={currentPage >= totalPages - 1} on:click={nextPage}>التالي</AppButton>
              </div>
            </div>
          {/if}
        </AppCard>
      </div>
    {/if}

    <!-- Info Card -->
    <div class="mt-6">
      <AppAlert intent="info" title="معلومات">
        يتم تحديث المخزون تلقائياً عند تأكيد طلبيات الموردين وتسجيل الاستهلاك اليومي. كل حركة مخزون يتم تسجيلها في سجل الحركات مع التفاصيل الكاملة.
      </AppAlert>
    </div>
  </div>
</Layout>
