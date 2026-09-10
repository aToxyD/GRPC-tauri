<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { createRuntimeScope, createTransientMessage } from "../lib/runtimeCleanup";
  import { createOperation } from "../lib/operationGuard";
  import { formatErrorMessage } from "../lib/errors";
  import { openFile, saveFile } from "../lib/tauri";
  import { importProductsPackage, exportStockMovementsPackage, importStockMovementsPackage, importContractCatalogPackage } from "../lib/contracts";
  import { getAllStocks, getStockSummary, getStockMovements, exportStockMovementsExcel, getSettings, getInventoryFifoView } from "../lib/contracts";
  import { showSuccess, showError } from "../lib/notifications";
  import type {
    InventoryStock,
    StockSummary,
    StockMovement,
    StockMovementFilters,
    StockMovementResponse,
    StockMovementType,
    Settings,
    InventoryStockPageView,
    InventoryProductView,
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

  const scope = createRuntimeScope();
  const summaryOp = createOperation({ scope });
  const fifoOp = createOperation({ scope });
  const movementsOp = createOperation({ scope });
  const setImportSuccessTransient = createTransientMessage(
    scope,
    (m) => (importSuccess = m),
  );
  onDestroy(() => scope.dispose());

  const summaryLoading = summaryOp.loading;
  const fifoLoading = fifoOp.loading;
  const movementsLoading = movementsOp.loading;

  // Section 2 & 3: Summary
  // @category ProjectionState
  let summary: StockSummary[] = $state([]);

  // Computed stats from summary
  // @category ProjectionState
  let totalProducts = $derived(summary.length);
  // @category ProjectionState
  let totalIn = $derived(summary.filter((p) => p.total_in > 0).length);
  // @category ProjectionState
  let totalOut = $derived(summary.filter((p) => p.total_out > 0).length);
  // @category ProjectionState
  let lowStockCount = $derived(
    summary.filter((p) => p.current_quantity < 10).length,
  );
  // @category ProjectionState
  let outOfStockCount = $derived(
    summary.filter((p) => p.current_quantity === 0).length,
  );

  // Section 4: Movements
  // @category ProjectionState
  let movements: StockMovement[] = $state([]);
  // @category UiState
  let movementsVisible = $state(false);
  // @category ProjectionState
  let totalMovements = $state(0);
  // @category UiState
  let currentPage = $state(0);
  // @category UiState
  let highlightedProductId = $state("");
  const PAGE_SIZE = 20;

  // Filters
  // @category UiState
  let filterProductId = $state("");
  // @category UiState
  let filterMovementType = $state("");
  // @category UiState
  let filterStartDate = $state("");
  // @category UiState
  let filterEndDate = $state("");

  // Messages (for import only)
  // @category TransientState
  let importError = $state("");
  // @category TransientState
  let importSuccess = $state("");
  // @category ProjectionState
  let settings: Settings | null = $state(null);

  // Fallback stocks
  // @category ProjectionState
  let stocks: InventoryStock[] = $state([]);

  // Section 2.5: FIFO Inventory View
  // @category ProjectionState
  let fifoView: InventoryStockPageView | null = $state(null);
  // @category UiState
  let expandedProductId: string | null = $state(null);

  function toggleFifoLayers(productId: string) {
    expandedProductId = expandedProductId === productId ? null : productId;
  }

  function getSourceLabel(sourceType: string | null): string {
    switch (sourceType?.toUpperCase()) {
      case "OPENING": return "مخزون إبتدائي";
      case "ORDER": return "طلبية";
      default: return sourceType || "-";
    }
  }

  onMount(async () => {
    try {
      settings = await getSettings();
    } catch (e) {
      console.error("Failed to load settings", e);
    }
    await loadSummary();
    await loadFifoView();
  });

  async function loadSummary() {
    await summaryOp.run(async () => {
      try {
        summary = await getStockSummary();
      } catch (e) {
        showError("خطأ في تحميل ملخص المخزون: " + formatErrorMessage(e));
        try {
          stocks = await getAllStocks();
        } catch (e2) {
          showError("خطأ في التحميل الاحتياطي: " + formatErrorMessage(e2));
        }
      }
    });
  }

  async function loadFifoView() {
    await fifoOp.run(async () => {
      try {
        fifoView = await getInventoryFifoView();
      } catch (e) {
        console.error("Failed to load FIFO view", e);
      }
    });
  }

  async function loadMovements() {
    movementsVisible = true;
    await movementsOp.run(async () => {
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
    });
  }

  async function showMovementsForProduct(productId: string) {
    filterProductId = productId;
    highlightedProductId = productId;
    currentPage = 0;
    await loadMovements(); // انتظر حتى يظهر القسم
    // ثم scroll
    scope.setTimeout(() => {
      document
        .getElementById("movements-section")
        ?.scrollIntoView({ behavior: "smooth" });
    }, 100);
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
        setImportSuccessTransient(`تم استيراد ${count} منتجات بنجاح`);
        await loadSummary();
      }
    } catch (e) {
      importError = "خطأ في الاستيراد: " + formatErrorMessage(e);
    }
  }

  async function handleImportContractCatalog() {
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
        const result = await importContractCatalogPackage(selected as string);
        setImportSuccessTransient(
          `تم استيراد كتالوج العقود بنجاح (${result.added} إضافة / ${result.updated} تحديث)`,
        );
      }
    } catch (e) {
      importError = "خطأ في استيراد كتالوج العقود: " + formatErrorMessage(e);
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

  // @category UiState — pagination
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

            <AppButton variant="secondary" on:click={handleImportContractCatalog} ariaLabel="استيراد كتالوج عقود التموين الرسمي من الولاية">
              <svg class="w-4 h-4 mr-2 inline-block text-amber-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04a11.367 11.367 0 01-1.091 5.496c.002.314.05.628.143.933a11.503 11.503 0 001.371 3.513c.176.326.362.641.551.944A12.026 12.026 0 0011.962 21.01a12.02 12.02 0 008.474-5.991c.401-.736.745-1.515 1.022-2.322a10.107 10.107 0 00.395-1.842 11.233 11.233 0 00-1.091-5.496z"/>
              </svg>
              استيراد كتالوج العقود
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

    <!-- SECTION 2.5: FIFO Inventory Cards -->
    {#if fifoView}
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
        <AppCard class="border-r-4 border-purple-500" padding="sm">
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400">قيمة المخزون (FIFO)</p>
              <p class="text-2xl font-bold text-purple-600 dark:text-purple-400">
                {fifoView.total_inventory_value.toLocaleString("ar-DZ", { maximumFractionDigits: 2 }) + " د.ج"}
              </p>
            </div>
            <div class="w-12 h-12 bg-purple-100 dark:bg-purple-900/50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-purple-600 dark:text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
              </svg>
            </div>
          </div>
        </AppCard>

        <AppCard class="border-r-4 border-indigo-500" padding="sm">
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400">عدد المنتجات</p>
              <p class="text-2xl font-bold text-indigo-600 dark:text-indigo-400">
                {fifoView.total_products.toLocaleString("ar-DZ")}
              </p>
            </div>
            <div class="w-12 h-12 bg-indigo-100 dark:bg-indigo-900/50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-indigo-600 dark:text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01"/>
              </svg>
            </div>
          </div>
        </AppCard>

        <AppCard class="border-r-4 border-teal-500" padding="sm">
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400">الطبقات النشطة</p>
              <p class="text-2xl font-bold text-teal-600 dark:text-teal-400">
                {fifoView.total_active_layers.toLocaleString("ar-DZ")}
              </p>
            </div>
            <div class="w-12 h-12 bg-teal-100 dark:bg-teal-900/50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-teal-600 dark:text-teal-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/>
              </svg>
            </div>
          </div>
        </AppCard>

        <AppCard class="border-r-4 border-rose-500" padding="sm">
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400">منتجات نفذت</p>
              <p class="text-2xl font-bold text-rose-600 dark:text-rose-400">
                {outOfStockCount.toLocaleString("ar-DZ")}
              </p>
            </div>
            <div class="w-12 h-12 bg-rose-100 dark:bg-rose-900/50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-rose-600 dark:text-rose-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636"/>
              </svg>
            </div>
          </div>
        </AppCard>

      </div>
    {/if}

    <!-- SEC-087 Phase 5: advisory snapshot-coverage warnings (display only) -->
    {#if fifoView && fifoView.warnings && fifoView.warnings.length > 0}
      <div class="mb-6 rounded-lg border border-amber-300 bg-amber-50 dark:border-amber-700 dark:bg-amber-900/30 p-4">
        <div class="flex items-start gap-3">
          <svg class="w-5 h-5 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
          </svg>
          <div>
            <p class="text-sm font-semibold text-amber-800 dark:text-amber-300">تنبيهات تغطية سندات الشراء</p>
            <ul class="mt-1 space-y-1">
              {#each fifoView.warnings as warning (warning.product_id)}
                <li class="text-sm text-amber-700 dark:text-amber-400">
                  <span class="font-medium">{warning.product_name}</span> — {warning.message}
                </li>
              {/each}
            </ul>
          </div>
        </div>
      </div>
    {/if}

    <!-- SECTION 3: Stock Table -->
    <div class="mb-8">
      <AppCard padding="none">
        <div class="p-4 border-b border-gray-200 dark:border-gray-700">
          <h3 class="font-semibold text-lg text-gray-800 dark:text-white">المخزون الحالي</h3>
        </div>

        <AppTable
          loading={$summaryLoading}
          empty={!$summaryLoading && summary.length === 0 && stocks.length === 0}
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
            <th class="table-header">قيمة المخزون</th>
            <th class="table-header">الطبقات</th>
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
            {@const fp = fifoView?.products.find(p => p.product_id === product.product_id)}
            <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors {isHighlighted ? 'bg-blue-50 dark:bg-blue-900/20' : ''}">
              <td class="table-cell font-medium">{product.product_name}</td>
              <td class="table-cell {product.current_quantity < 10 ? 'text-red-700 dark:text-red-400 font-bold' : ''}">
                {product.current_quantity.toFixed(2)}
              </td>
              <td class="table-cell text-sm font-medium text-purple-700 dark:text-purple-400">
                {fp ? fp.total_value.toFixed(2) + " د.ج" : "-"}
              </td>
              <td class="table-cell">
                {#if fp && fp.layer_count > 0}
                  <AppButton variant="ghost" size="sm" class="text-teal-600 hover:text-teal-800 dark:text-teal-400 dark:hover:text-teal-300 underline" on:click={() => toggleFifoLayers(product.product_id)}>
                    {fp.layer_count} {fp.layer_count > 1 ? "طبقات" : "طبقة"}
                    {expandedProductId === product.product_id ? "▲" : "▼"}
                  </AppButton>
                {:else}
                  <span class="text-gray-400">0</span>
                {/if}
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
            {#if fp && expandedProductId === product.product_id && fp.layers.length > 0}
              <tr class="bg-gray-50 dark:bg-gray-800/50">
                <td colspan="10" class="p-0">
                  <div class="px-6 py-3">
                    <table class="w-full text-sm">
                      <thead>
                        <tr class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                          <th class="px-3 py-1 text-right">المصدر</th>
                          <th class="px-3 py-1 text-right">التاريخ</th>
                          <th class="px-3 py-1 text-right">الكمية</th>
                          <th class="px-3 py-1 text-right">السعر</th>
                          <th class="px-3 py-1 text-right">القيمة</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each fp.layers as layer}
                          <tr class="border-t border-gray-200 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-700/30">
                            <td class="px-3 py-1 text-right font-medium">{getSourceLabel(layer.source_type)}</td>
                            <td class="px-3 py-1 text-right text-gray-600 dark:text-gray-400">{formatDate(layer.received_at)}</td>
                            <td class="px-3 py-1 text-right">{layer.qty_remaining.toFixed(2)}</td>
                            <td class="px-3 py-1 text-right">{layer.unit_cost.toFixed(2) + " د.ج"}</td>
                            <td class="px-3 py-1 text-right font-medium text-purple-700 dark:text-purple-400">{layer.layer_value.toFixed(2) + " د.ج"}</td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                </td>
              </tr>
            {/if}
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
                <option value="OPENING">مخزون إبتدائي</option>
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
            loading={$movementsLoading}
            empty={!$movementsLoading && movements.length === 0}
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
              <th class="table-header">السعر</th>
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
                <td class="table-cell text-sm font-medium text-gray-800 dark:text-gray-200">
                  {movement.unit_cost !== null && movement.unit_cost !== undefined ? movement.unit_cost.toFixed(2) + " د.ج" : "-"}
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
          {#if !$movementsLoading && movements.length > 0}
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
