<script lang="ts">
  import { onMount } from 'svelte';
  import { getSettings, getAllStocks, listSupplierOrders } from '../lib/tauri';
  import type { Settings, InventoryStock, SupplierOrder } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let settings: Settings | null = null;
  let stocks: InventoryStock[] = [];
  let orders: SupplierOrder[] = [];
  let loading = true;

  onMount(async () => {
    try {
      [stocks, orders, settings] = await Promise.all([
        getAllStocks(),
        listSupplierOrders(),
        getSettings()
      ]);
    } catch (e) {
      // Error loading data
    } finally {
      loading = false;
    }
  });

  $: confirmedOrders = orders.filter(o => o.status === 'Confirmed' || o.status === 'Received');
  $: lowStockItems = stocks.filter(s => s.quantity < 10);
  $: subtitle = settings?.unit_name || 'مطعم الوحدة';
</script>

<Layout nodeType="UNIT" title="لوحة تحكم الوحدة" {subtitle}>

  {#if loading}
    <div class="flex items-center justify-center h-64">
      <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-civil-blue"></div>
    </div>
  {:else}
    <!-- Stats Cards -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
      <div class="card">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 mb-1">المنتجات في المخزون</p>
            <p class="text-3xl font-bold text-gray-800">{stocks.length}</p>
          </div>
          <div class="w-12 h-12 bg-blue-50 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-civil-blue" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/>
            </svg>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 mb-1">الطلبيات المؤكدة</p>
            <p class="text-3xl font-bold text-gray-800">{confirmedOrders.length}</p>
          </div>
          <div class="w-12 h-12 bg-green-50 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-green-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 mb-1">مخزون منخفض</p>
            <p class="text-3xl font-bold {lowStockItems.length > 0 ? 'text-red-600' : 'text-gray-800'}">{lowStockItems.length}</p>
          </div>
          <div class="w-12 h-12 {lowStockItems.length > 0 ? 'bg-red-50' : 'bg-gray-50'} rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 {lowStockItems.length > 0 ? 'text-red-500' : 'text-gray-400'}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
            </svg>
          </div>
        </div>
      </div>
    </div>

    <!-- Stock Status -->
    <div class="card mb-8">
      <div class="flex items-center justify-between mb-6">
        <h2 class="text-xl font-semibold text-gray-800">حالة المخزون</h2>
        <a href="#/unit/stock" class="text-civil-blue hover:underline text-sm">عرض الكل</a>
      </div>

      {#if stocks.length === 0}
        <div class="text-center py-8 text-gray-500">
          <p>لا يوجد منتجات في المخزون</p>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">المنتج</th>
                <th class="table-header">الكمية</th>
                <th class="table-header">الوحدة</th>
                <th class="table-header">آخر تحديث</th>
              </tr>
            </thead>
            <tbody>
              {#each stocks.slice(0, 5) as stock}
                <tr class="hover:bg-gray-50 {stock.quantity < 10 ? 'bg-red-50' : ''}">
                  <td class="table-cell font-medium">{stock.product_name}</td>
                  <td class="table-cell">
                    <span class="{stock.quantity < 10 ? 'text-red-600 font-semibold' : ''}">
                      {stock.quantity.toFixed(2)}
                    </span>
                  </td>
                  <td class="table-cell">{stock.unit}</td>
                  <td class="table-cell">{new Date(stock.last_updated).toLocaleDateString('fr-FR')}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <!-- Recent Orders -->
    <div class="card">
      <div class="flex items-center justify-between mb-6">
        <h2 class="text-xl font-semibold text-gray-800">الطلبيات الأخيرة</h2>
        <a href="#/unit/orders" class="text-civil-blue hover:underline text-sm">عرض الكل</a>
      </div>

      {#if orders.length === 0}
        <div class="text-center py-8 text-gray-500">
          <p>لا يوجد طلبيات مسجلة</p>
          <a href="#/unit/orders" class="text-civil-blue hover:underline mt-2 inline-block">إنشاء طلبية</a>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">المورد</th>
                <th class="table-header">التاريخ</th>
                <th class="table-header">المبلغ</th>
                <th class="table-header">الحالة</th>
              </tr>
            </thead>
            <tbody>
              {#each orders.slice(0, 5) as order}
                <tr class="hover:bg-gray-50">
                  <td class="table-cell font-medium">{order.supplier_name}</td>
                  <td class="table-cell">{new Date(order.order_date).toLocaleDateString('fr-FR')}</td>
                  <td class="table-cell">{order.total_amount?.toFixed(2) || '-'} DA</td>
                  <td class="table-cell">
                    <span class="px-2 py-1 rounded-full text-xs font-medium
                      {order.status === 'Confirmed' ? 'bg-green-100 text-green-800' : 
                       order.status === 'Received' ? 'bg-blue-100 text-blue-800' :
                       order.status === 'Cancelled' ? 'bg-red-100 text-red-800' :
                       'bg-gray-100 text-gray-800'}">
                      {order.status}
                    </span>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</Layout>
