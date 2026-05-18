<script lang="ts">
  import { onMount } from "svelte";
  import {
    listUnits,
    listProducts,
    getSettings,
    getSystemMetrics,
    syncPreflightCheck,
  } from "../lib/tauri";
  import type { Unit, Product, Settings, SystemMetrics, SyncPreflightCheck } from "../lib/types";
  import Layout from "../components/Layout.svelte";

  let units: Unit[] = [];
  let products: Product[] = [];
  let settings: Settings | null = null;
  let loading = true;
  let currentYear = new Date().getFullYear();
  let currentMonth = new Date().getMonth() + 1;
  let reportsThisMonth = 0;

  // Stats Data
  let metrics: SystemMetrics | null = null;
  let preflight: SyncPreflightCheck | null = null;
  let loadingStats = true;
  let statsError = "";

  onMount(async () => {
    loadData();
    loadStats();
  });

  async function loadStats() {
    try {
      loadingStats = true;
      [metrics, preflight] = await Promise.all([
        getSystemMetrics(),
        syncPreflightCheck(),
      ]);
      reportsThisMonth = metrics.monthly_reports;
    } catch (err) {
      statsError = err instanceof Error ? err.message : String(err);
    } finally {
      loadingStats = false;
    }
  }

  async function loadData() {
    try {
      loading = true;
      settings = await getSettings();
      const wilayaCode = settings?.wilaya_code || "";

      [units, products] = await Promise.all([
        listUnits(wilayaCode).catch(() => []),
        listProducts(),
      ]);

      if (settings) {
        currentYear = settings.current_year;
      }
    } catch (e) {
      // Error loading data
    } finally {
      loading = false;
    }
  }

  $: currentYearProducts = products.filter((p) => p.year === currentYear);
  $: subtitle = settings?.wilaya_name || "مديرية الولاية";
</script>

<Layout nodeType="WILAYA" title="لوحة تحكم الولاية" {subtitle}>
  {#if loading}
    <div class="flex items-center justify-center h-64">
      <div
        class="animate-spin rounded-full h-12 w-12 border-b-2 border-civil-blue"
      ></div>
    </div>
  {:else}
    <div class="card mb-8">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">
          إحصائيات النظام العامة
        </h2>
      </div>

      {#if loadingStats}
        <div class="flex items-center justify-center h-24">
          <div
            class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"
          ></div>
        </div>
      {:else if statsError}
        <div class="bg-red-50 text-red-600 p-4 rounded-lg">
          <p>حدث خطأ أثناء تحميل الإحصائيات: {statsError}</p>
        </div>
      {:else}
        <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
          <div
            class="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-900 rounded-lg border border-gray-100 dark:border-gray-700"
          >
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">إجمالي المنتجات</p>
              <p class="text-2xl font-bold text-gray-800 dark:text-gray-100">{metrics?.total_products || 0}</p>
            </div>
            <div
              class="w-10 h-10 bg-blue-100 rounded-lg flex items-center justify-center"
            >
              <svg
                class="w-5 h-5 text-civil-blue"
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

          <div
            class="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-900 rounded-lg border border-gray-100 dark:border-gray-700"
          >
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">إجمالي التقارير</p>
              <p class="text-2xl font-bold text-gray-800 dark:text-gray-100">{metrics?.daily_reports || 0}</p>
            </div>
            <div
              class="w-10 h-10 bg-green-100 rounded-lg flex items-center justify-center"
            >
              <svg
                class="w-5 h-5 text-green-600"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                />
              </svg>
            </div>
          </div>

          <div
            class="flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-900 rounded-lg border border-gray-100 dark:border-gray-700"
          >
            <div>
              <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">المستخدمين النشطين</p>
              <p class="text-2xl font-bold text-gray-800 dark:text-gray-100">{metrics?.active_users || 0}</p>
            </div>
            <div
              class="w-10 h-10 bg-purple-100 rounded-lg flex items-center justify-center"
            >
              <svg
                class="w-5 h-5 text-purple-600"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"
                />
              </svg>
            </div>
          </div>
        </div>
      {/if}
    </div>

    {#if preflight}
      <div class="card mb-8">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">جاهزية أمان المزامنة</h2>
          <span
            class="px-3 py-1 rounded-full text-sm font-medium
            {preflight.status === 'ok' ? 'bg-green-100 text-green-800' : ''}
            {preflight.status === 'warn' ? 'bg-yellow-100 text-yellow-800' : ''}
            {preflight.status === 'fail' ? 'bg-red-100 text-red-800' : ''}"
          >
            {preflight.status === 'ok' ? 'جاهز' : preflight.status === 'warn' ? 'تحذير' : 'فشل'}
          </span>
        </div>
        <div class="space-y-2">
          {#each preflight.reason_messages_ar as message}
            <div class="text-sm text-gray-700 dark:text-gray-100">- {message}</div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Stats Cards -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
      <div class="card">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">التقارير هذا الشهر</p>
            <p class="text-3xl font-bold text-gray-800 dark:text-gray-100">{reportsThisMonth}</p>
          </div>
          <div
            class="w-12 h-12 bg-blue-50 dark:bg-blue-900/20 rounded-lg flex items-center justify-center"
          >
            <svg
              class="w-6 h-6 text-civil-blue"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
              />
            </svg>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">المنتجات المسجلة</p>
            <p class="text-3xl font-bold text-gray-800 dark:text-gray-100">
              {currentYearProducts.length}
            </p>
          </div>
          <div
            class="w-12 h-12 bg-green-50 rounded-lg flex items-center justify-center"
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
                d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
              />
            </svg>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">السنة الحالية</p>
            <p class="text-3xl font-bold text-gray-800 dark:text-gray-100">{currentYear}</p>
          </div>
          <div
            class="w-12 h-12 bg-purple-50 rounded-lg flex items-center justify-center"
          >
            <svg
              class="w-6 h-6 text-purple-600"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
              />
            </svg>
          </div>
        </div>
      </div>
    </div>

    <!-- Recent Units -->
    <div class="card mb-8">
      <div class="flex items-center justify-between mb-6">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">قائمة الوحدات</h2>
        <a href="/wilaya/units" class="text-civil-blue hover:underline text-sm"
          >إدارة الوحدات</a
        >
      </div>

      {#if units.length === 0}
        <div class="text-center py-8 text-gray-500 dark:text-gray-400">
          <svg
            class="w-12 h-12 mx-auto mb-3 text-gray-300"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"
            />
          </svg>
          <p class="text-lg">لا يوجد وحدات مسجلة</p>
          <a
            href="/wilaya/units"
            class="text-civil-blue hover:underline mt-2 inline-block"
            >إنشاء وحدة</a
          >
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">الرمز</th>
                <th class="table-header">الاسم</th>
                <th class="table-header">تاريخ الإنشاء</th>
              </tr>
            </thead>
            <tbody>
              {#each units.slice(0, 5) as unit}
                <tr class="hover:bg-gray-50 dark:bg-gray-900">
                  <td class="table-cell font-medium">{unit.code}</td>
                  <td class="table-cell">{unit.name}</td>
                  <td class="table-cell"
                    >{new Date(unit.created_at).toLocaleDateString("ar-EG")}</td
                  >
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <!-- Recent Products -->
    <div class="card">
      <div class="flex items-center justify-between mb-6">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">المنتجات المتاحة</h2>
        <a
          href="/wilaya/products"
          class="text-civil-blue hover:underline text-sm">إدارة المنتجات</a
        >
      </div>

      {#if currentYearProducts.length === 0}
        <div class="text-center py-8 text-gray-500 dark:text-gray-400">
          <svg
            class="w-12 h-12 mx-auto mb-3 text-gray-300"
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
          <p class="text-lg">لا يوجد منتجات مسجلة</p>
          <a
            href="/wilaya/products"
            class="text-civil-blue hover:underline mt-2 inline-block"
            >إضافة منتجات</a
          >
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">الاسم</th>
                <th class="table-header">السعر</th>
                <th class="table-header">الضريبة</th>
                <th class="table-header">المورد</th>
              </tr>
            </thead>
            <tbody>
              {#each currentYearProducts.slice(0, 5) as product}
                <tr class="hover:bg-gray-50 dark:bg-gray-900">
                  <td class="table-cell font-medium">{product.name}</td>
                  <td class="table-cell">{product.base_price.toFixed(2)} دج</td>
                  <td class="table-cell">{product.tva}%</td>
                  <td class="table-cell">{product.supplier_name || "-"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</Layout>
