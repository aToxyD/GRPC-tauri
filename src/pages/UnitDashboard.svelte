<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listSupplierOrders } from '../lib/contracts';
  import { getSettings, getAllStocks } from '../lib/contracts';
  import type { Settings, InventoryStock, SupplierOrder } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { unitLabel } from '../lib/unitLabels';

  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';

  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation } from '../lib/operationGuard';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const dashboardOp = createOperation({ scope });
  const loading = dashboardOp.loading;

  // @category ProjectionState
  let settings: Settings | null = null;
  // @category ProjectionState
  let stocks: InventoryStock[] = [];
  // @category ProjectionState
  let orders: SupplierOrder[] = [];

  onMount(async () => {
    await dashboardOp.run(async () => {
      [stocks, orders, settings] = await Promise.all([
        getAllStocks(),
        listSupplierOrders(),
        getSettings()
      ]);
    });
  });

  // @category UiState
  $: confirmedOrders = orders.filter(o => o.status === 'Confirmed');
  // @category UiState
  $: lowStockItems = stocks.filter(s => s.quantity < 10);
  // @category UiState
  $: subtitle = settings?.unit_name || 'مطعم الوحدة';

  function getOrderStatusIntent(status: string): 'success' | 'warning' | 'danger' | 'neutral' | 'info' {
    switch (status) {
      case 'Confirmed': return 'success';
      case 'Draft': return 'warning';
      default: return 'neutral';
    }
  }
</script>

<Layout nodeType="UNIT" title="لوحة تحكم الوحدة" {subtitle}>

  {#if $loading}
    <AppLoadingState message="جارٍ التحميل..." />
  {:else}
    <!-- Stats Cards -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
      <AppCard class="border-t-4 border-blue-500">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">المنتجات في المخزون</p>
            <p class="text-3xl font-bold text-gray-800 dark:text-gray-100">{stocks.length}</p>
          </div>
          <div class="w-12 h-12 bg-blue-50 dark:bg-blue-900/20 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-civil-blue dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/>
            </svg>
          </div>
        </div>
      </AppCard>

      <AppCard class="border-t-4 border-green-500">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">الطلبيات المؤكدة</p>
            <p class="text-3xl font-bold text-gray-800 dark:text-gray-100">{confirmedOrders.length}</p>
          </div>
          <div class="w-12 h-12 bg-green-50 dark:bg-green-900/20 rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
          </div>
        </div>
      </AppCard>

      <AppCard class="border-t-4 border-red-500">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm text-gray-500 dark:text-gray-400 mb-1">مخزون منخفض</p>
            <p class="text-3xl font-bold {lowStockItems.length > 0 ? 'text-red-600 dark:text-red-400' : 'text-gray-800 dark:text-gray-100'}">{lowStockItems.length}</p>
          </div>
          <div class="w-12 h-12 {lowStockItems.length > 0 ? 'bg-red-50 dark:bg-red-900/20' : 'bg-gray-50 dark:bg-gray-900'} rounded-lg flex items-center justify-center">
            <svg class="w-6 h-6 {lowStockItems.length > 0 ? 'text-red-500 dark:text-red-400' : 'text-gray-400'}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
            </svg>
          </div>
        </div>
      </AppCard>
    </div>

    <!-- Stock Status -->
    <AppCard padding="none" class="mb-8">
      <div class="p-4 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">حالة المخزون</h2>
        <a href="#/unit/stock" class="text-civil-blue hover:underline text-sm">عرض الكل</a>
      </div>

      <AppTable empty={stocks.length === 0}>
        <svelte:fragment slot="empty">
          <AppEmptyState
            title="لا يوجد منتجات في المخزون"
            description="أنشئ أول منتج أو قم بالاستيراد"
            icon="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
          />
        </svelte:fragment>
        
        <svelte:fragment slot="head">
          <th class="table-header">المنتج</th>
          <th class="table-header">الكمية</th>
          <th class="table-header">الوحدة</th>
          <th class="table-header">آخر تحديث</th>
        </svelte:fragment>

        {#each stocks.slice(0, 5) as stock}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors {stock.quantity < 10 ? 'bg-red-50/50 dark:bg-red-900/10' : ''}">
            <td class="table-cell font-medium">{stock.product_name}</td>
            <td class="table-cell">
              <span class="{stock.quantity < 10 ? 'text-red-600 dark:text-red-400 font-semibold' : ''}">
                {stock.quantity.toFixed(2)}
              </span>
            </td>
            <td class="table-cell">{unitLabel(stock.consumption_unit)}</td>
            <td class="table-cell text-sm text-gray-500 dark:text-gray-400">{new Date(stock.last_updated).toLocaleDateString('fr-FR')}</td>
          </tr>
        {/each}
      </AppTable>
    </AppCard>

    <!-- Recent Orders -->
    <AppCard padding="none">
      <div class="p-4 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">الطلبيات الأخيرة</h2>
        <a href="#/unit/orders" class="text-civil-blue hover:underline text-sm">عرض الكل</a>
      </div>

      <AppTable empty={orders.length === 0}>
        <svelte:fragment slot="empty">
          <AppEmptyState
            title="لا يوجد طلبيات مسجلة"
            description="إنشاء طلبية جديدة لإضافة المنتجات إلى المخزون"
            icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          >
            <svelte:fragment slot="action">
              <a href="#/unit/orders" class="text-civil-blue hover:underline mt-2 inline-block">إنشاء طلبية</a>
            </svelte:fragment>
          </AppEmptyState>
        </svelte:fragment>
        
        <svelte:fragment slot="head">
          <th class="table-header">المورد</th>
          <th class="table-header">التاريخ</th>
          <th class="table-header">المبلغ</th>
          <th class="table-header">الحالة</th>
        </svelte:fragment>

        {#each orders.slice(0, 5) as order}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
            <td class="table-cell font-medium">{order.supplier_name}</td>
            <td class="table-cell text-sm text-gray-500 dark:text-gray-400">{new Date(order.order_date).toLocaleDateString('fr-FR')}</td>
            <td class="table-cell font-medium">{order.total_amount?.toFixed(2) || '-'} DA</td>
            <td class="table-cell">
              <AppBadge intent={getOrderStatusIntent(order.status)} size="sm">
                {order.status}
              </AppBadge>
            </td>
          </tr>
        {/each}
      </AppTable>
    </AppCard>
  {/if}
</Layout>
