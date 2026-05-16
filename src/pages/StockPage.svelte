<script lang="ts">
  import { onMount } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import {
    getAllStocks,
    importProductsPackage,
    getStockSummary,
    getStockMovements,
    exportStockMovementsExcel,
    exportStockMovementsPackage,
    importStockMovementsPackage,
    getSettings,
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
      showError("خطأ في تحميل ملخص المخزون: " + String(e));
      // Fallback to old method
      try {
        stocks = await getAllStocks();
      } catch (e2) {
        showError("خطأ في التحميل الاحتياطي: " + String(e2));
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
      showError("خطأ في تحميل حركات المخزون: " + String(e));
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
    setTimeout(() => {
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
      const filePath = await save({
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
      showError("خطأ في التصدير: " + String(e));
    }
  }

  async function handleImportProducts() {
    try {
      importError = "";
      importSuccess = "";

      const selected = await open({
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
        setTimeout(() => (importSuccess = ""), 3000);
      }
    } catch (e) {
      importError = "خطأ في الاستيراد: " + String(e);
    }
  }


  async function handleExportMovementsPackage() {
    try {
      const now = new Date();
      const dateStr = now.toISOString().split("T")[0];
      const unitCode = settings?.unit_code || "UNIT";
      const defaultFilename = `حزمة_حركة_المخزون_${unitCode}_${dateStr}.sync`;

      const filePath = await save({
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
      showError("خطأ في تصدير حزمة حركات المخزون: " + String(e));
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

  function getStatusBadge(quantity: number) {
    if (quantity === 0) {
      return { text: "نفد", class: "bg-gray-100 text-gray-600" };
    } else if (quantity < 10) {
      return { text: "منخفض", class: "bg-red-100 text-red-600" };
    } else if (quantity < 50) {
      return { text: "متوسط", class: "bg-orange-100 text-orange-600" };
    } else {
      return { text: "جيد", class: "bg-green-100 text-green-600" };
    }
  }

  function getMovementTypeBadge(type: StockMovementType) {
    switch (type) {
      case "IN":
        return { text: "↑ دخول", class: "bg-green-100 text-green-600" };
      case "OUT":
        return { text: "↓ خروج", class: "bg-red-100 text-red-600" };
      case "OPENING":
        return { text: "◉ افتتاحي", class: "bg-gray-100 text-gray-600" };
      default:
        return { text: type, class: "bg-gray-100 text-gray-600" };
    }
  }

  function getMovementQuantityColor(type: StockMovementType) {
    return type === "IN" || type === "OPENING"
      ? "text-green-600"
      : "text-red-600";
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
  <!-- SECTION 1: Header -->
  <div class="mb-8">
    <div class="flex items-center justify-end gap-3">
      <button
        onclick={handleExportMovements}
        class="btn-secondary flex items-center gap-2"
      >
        <svg
          class="w-4 h-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          />
        </svg>
        <span>تصدير حركات المخزون (Excel)</span>
      </button>


      <div class="h-8 w-px bg-gray-200 mx-1"></div>

      <button
        onclick={handleExportMovementsPackage}
        class="btn-secondary flex items-center gap-2 bg-blue-50 border-blue-200 hover:bg-blue-100"
        title="تصدير حركات المخزون كحزمة مشفرة وآمنة للمزامنة مع الولاية"
      >
        <svg
          class="w-4 h-4 text-blue-600"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
          />
        </svg>
        <span class="text-blue-700">تصدير حزمة حركات (.sync)</span>
      </button>

      <div class="h-8 w-px bg-gray-200 mx-1"></div>

      <button
        onclick={handleImportProducts}
        class="btn-secondary flex items-center gap-2"
        title="استيراد حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد"
      >
        <svg
          class="w-4 h-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
          />
        </svg>
        <span>استيراد منتجات الولاية</span>
      </button>
    </div>
  </div>

  {#if importError}
    <div
      class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm"
    >
      {importError}
    </div>
  {/if}

  {#if importSuccess}
    <div
      class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm"
    >
      {importSuccess}
    </div>
  {/if}

  <!-- SECTION 2: Summary Cards -->
  <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
    <!-- Card 1: Total Products -->
    <div class="card p-4 border-l-4 border-blue-500">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-sm text-gray-500">إجمالي المنتجات</p>
          <p class="text-2xl font-bold text-blue-600">
            {totalProducts.toLocaleString("ar-DZ")}
          </p>
        </div>
        <div
          class="w-12 h-12 bg-blue-100 rounded-lg flex items-center justify-center"
        >
          <svg
            class="w-6 h-6 text-blue-600"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
            />
          </svg>
        </div>
      </div>
    </div>

    <!-- Card 2: Total In -->
    <div class="card p-4 border-l-4 border-green-500">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-sm text-gray-500">إجمالي الدخول</p>
          <p class="text-2xl font-bold text-green-600">
            {totalIn.toLocaleString("ar-DZ")}
          </p>
        </div>
        <div
          class="w-12 h-12 bg-green-100 rounded-lg flex items-center justify-center"
        >
          <svg
            class="w-6 h-6 text-green-600"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M5 10l7-7m0 0l7 7m-7-7v18"
            />
          </svg>
        </div>
      </div>
    </div>

    <!-- Card 3: Total Out -->
    <div class="card p-4 border-l-4 border-red-500">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-sm text-gray-500">إجمالي الخروج</p>
          <p class="text-2xl font-bold text-red-600">
            {totalOut.toLocaleString("ar-DZ")}
          </p>
        </div>
        <div
          class="w-12 h-12 bg-red-100 rounded-lg flex items-center justify-center"
        >
          <svg
            class="w-6 h-6 text-red-600"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 14l-7 7m0 0l-7-7m7 7V3"
            />
          </svg>
        </div>
      </div>
    </div>

    <!-- Card 4: Low Stock -->
    <div class="card p-4 border-l-4 border-orange-500">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-sm text-gray-500">منتجات منخفضة</p>
          <p class="text-2xl font-bold text-orange-600">
            {lowStockCount.toLocaleString("ar-DZ")}
          </p>
        </div>
        <div
          class="w-12 h-12 bg-orange-100 rounded-lg flex items-center justify-center"
        >
          <svg
            class="w-6 h-6 text-orange-600"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
            />
          </svg>
        </div>
      </div>
    </div>
  </div>

  <!-- SECTION 3: Stock Table -->
  <div class="card mb-8">
    <div class="p-4 border-b border-gray-200">
      <h3 class="font-semibold text-lg">المخزون الحالي</h3>
    </div>

    {#if summaryLoading}
      <div class="flex items-center justify-center py-12">
        <div
          class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"
        ></div>
      </div>
    {:else if summary.length === 0 && stocks.length === 0}
      <div class="text-center py-12 text-gray-500">
        <svg
          class="w-16 h-16 mx-auto mb-4 text-gray-300"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
          />
        </svg>
        <p class="text-lg">لا يوجد منتجات في المخزون</p>
        <p class="text-sm mt-2">استورد قائمة منتجات الولاية</p>
      </div>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full">
          <thead>
            <tr>
              <th class="table-header">المنتج</th>
              <th class="table-header">الكمية الحالية</th>
              <th class="table-header">إجمالي الدخول</th>
              <th class="table-header">إجمالي الخروج</th>
              <th class="table-header">عدد الحركات</th>
              <th class="table-header">آخر حركة</th>
              <th class="table-header">الحالة</th>
              <th class="table-header">إجراءات</th>
            </tr>
          </thead>
          <tbody>
            {#each summary as product}
              {@const status = getStatusBadge(product.current_quantity)}
              {@const isHighlighted =
                highlightedProductId === product.product_id}
              <tr class="hover:bg-gray-50 {isHighlighted ? 'bg-blue-50' : ''}">
                <td class="table-cell font-medium">{product.product_name}</td>
                <td
                  class="table-cell {product.current_quantity < 10
                    ? 'text-red-700 font-bold'
                    : ''}"
                >
                  {product.current_quantity.toFixed(2)}
                </td>
                <td class="table-cell text-green-600"
                  >{product.total_in.toFixed(2)}</td
                >
                <td class="table-cell text-red-600"
                  >{product.total_out.toFixed(2)}</td
                >
                <td class="table-cell">{product.movement_count}</td>
                <td class="table-cell text-sm text-gray-500">
                  {product.last_movement
                    ? formatDate(product.last_movement)
                    : "-"}
                </td>
                <td class="table-cell">
                  <span
                    class="px-2 py-1 rounded-full text-xs font-medium {status.class}"
                  >
                    {status.text}
                  </span>
                </td>
                <td class="table-cell">
                  <div class="flex gap-2">
                    <button
                      onclick={() =>
                        showMovementsForProduct(product.product_id)}
                      class="text-blue-600 hover:text-blue-800 text-sm font-medium"
                    >
                      الحركات
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>

  <!-- SECTION 4: Movements Table -->
  {#if movementsVisible}
    <div id="movements-section" class="card mb-8">
      <div class="p-4 border-b border-gray-200">
        <h3 class="font-semibold text-lg">سجل حركات المخزون</h3>
      </div>

      <!-- Filters -->
      <div class="p-4 bg-gray-50 border-b border-gray-200">
        <div class="grid grid-cols-1 md:grid-cols-4 gap-4 mb-4">
          <!-- Filter 1: Product -->
          <div>
            <label
              for="filter-product"
              class="block text-sm font-medium text-gray-700 mb-1">المنتج</label
            >
            <select
              id="filter-product"
              bind:value={filterProductId}
              class="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
            >
              <option value="">كل المنتجات</option>
              {#each summary as product}
                <option value={product.product_id}
                  >{product.product_name}</option
                >
              {/each}
            </select>
          </div>

          <!-- Filter 2: Movement Type -->
          <div>
            <label
              for="filter-type"
              class="block text-sm font-medium text-gray-700 mb-1"
              >نوع الحركة</label
            >
            <select
              id="filter-type"
              bind:value={filterMovementType}
              class="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
            >
              <option value="">كل الأنواع</option>
              <option value="IN">دخول مخزون</option>
              <option value="OUT">خروج مخزون</option>
              <option value="OPENING">رصيد افتتاحي</option>
            </select>
          </div>

          <!-- Filter 3: Start Date -->
          <div>
            <label
              for="filter-start"
              class="block text-sm font-medium text-gray-700 mb-1"
              >من تاريخ</label
            >
            <input
              id="filter-start"
              type="date"
              bind:value={filterStartDate}
              class="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
            />
          </div>

          <!-- Filter 4: End Date -->
          <div>
            <label
              for="filter-end"
              class="block text-sm font-medium text-gray-700 mb-1"
              >إلى تاريخ</label
            >
            <input
              id="filter-end"
              type="date"
              bind:value={filterEndDate}
              class="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm"
            />
          </div>
        </div>

        <div class="flex gap-3">
          <button onclick={loadMovements} class="btn-primary">بحث</button>
          <button onclick={clearFilters} class="btn-secondary">مسح</button>
        </div>
      </div>

      <!-- Movements Table -->
      <div class="overflow-x-auto">
        {#if movementsLoading}
          <div class="flex items-center justify-center py-12">
            <div
              class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"
            ></div>
          </div>
        {:else if movements.length === 0}
          <div class="text-center py-8 text-gray-500">
            <p>لا توجد حركات تطابق الفلاتر المحددة</p>
          </div>
        {:else}
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">التاريخ والوقت</th>
                <th class="table-header">المنتج</th>
                <th class="table-header">نوع الحركة</th>
                <th class="table-header">الكمية</th>
                <th class="table-header">الرصيد قبل</th>
                <th class="table-header">الرصيد بعد</th>
                <th class="table-header">المرجع</th>
                <th class="table-header">المستخدم</th>
              </tr>
            </thead>
            <tbody>
              {#each movements as movement}
                {@const typeBadge = getMovementTypeBadge(
                  movement.movement_type,
                )}
                {@const qtyColor = getMovementQuantityColor(
                  movement.movement_type,
                )}
                {@const refShort = movement.reference_id
                  ? movement.reference_id.slice(0, 8)
                  : "-"}
                <tr class="hover:bg-gray-50">
                  <td class="table-cell text-sm"
                    >{formatDate(movement.timestamp)}</td
                  >
                  <td class="table-cell font-medium"
                    >{movement.product_name || "-"}</td
                  >
                  <td class="table-cell">
                    <span
                      class="px-2 py-1 rounded-full text-xs font-medium {typeBadge.class}"
                    >
                      {typeBadge.text}
                    </span>
                  </td>
                  <td class="table-cell font-medium {qtyColor}">
                    {movement.movement_type === "OUT"
                      ? "-"
                      : "+"}{movement.quantity.toFixed(2)}
                  </td>
                  <td class="table-cell text-gray-500"
                    >{movement.balance_before.toFixed(2)}</td
                  >
                  <td class="table-cell text-gray-500"
                    >{movement.balance_after.toFixed(2)}</td
                  >
                  <td class="table-cell text-sm">
                    {#if movement.reference_type}
                      <span class="text-gray-600"
                        >{movement.reference_type}:</span
                      >
                      <span class="font-mono text-xs">{refShort}</span>
                    {:else}
                      <span class="text-gray-400">-</span>
                    {/if}
                  </td>
                  <td class="table-cell text-sm">{movement.username}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

      <!-- Pagination -->
      {#if !movementsLoading && movements.length > 0}
        <div
          class="p-4 border-t border-gray-200 flex items-center justify-between"
        >
          <p class="text-sm text-gray-600">
            الصفحة {currentPage + 1} من {totalPages || 1} | إجمالي: {totalMovements.toLocaleString(
              "ar-DZ",
            )} حركة
          </p>
          <div class="flex gap-2">
            <button
              onclick={prevPage}
              disabled={currentPage === 0}
              class="btn-secondary text-sm disabled:opacity-50 disabled:cursor-not-allowed"
            >
              السابق
            </button>
            <button
              onclick={nextPage}
              disabled={currentPage >= totalPages - 1}
              class="btn-secondary text-sm disabled:opacity-50 disabled:cursor-not-allowed"
            >
              التالي
            </button>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Info Card -->
  <div class="mt-6 card bg-blue-50 border-blue-200">
    <h3 class="font-semibold text-blue-800 mb-2">معلومات</h3>
    <p class="text-sm text-blue-700">
      يتم تحديث المخزون تلقائياً عند تأكيد طلبيات الموردين وتسجيل الاستهلاك
      اليومي. كل حركة مخزون يتم تسجيلها في سجل الحركات مع التفاصيل الكاملة.
    </p>
  </div>
</Layout>
