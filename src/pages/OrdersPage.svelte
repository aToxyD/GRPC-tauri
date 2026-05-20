<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatErrorMessage } from '../lib/errors';
  import {
    listSupplierOrders,
    createSupplierOrder,
    updateSupplierOrder,
    deleteSupplierOrder,
    confirmOrder,
    listProducts,
    getSupplierOrderItems,
    showAsk,
  } from '../lib/tauri';
  import type { SupplierOrder, Product, OrderItemInput, SupplierOrderItem } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  const ordersOp = createOperation({ scope });
  const loading = ordersOp.loading;
  const error = ordersOp.error;

  let orders = $state<SupplierOrder[]>([]);
  let products = $state<Product[]>([]);
  let showModal = $state(false);
  let editingOrderId = $state<string | null>(null);
  let success = $state('');
  let selectedOrder = $state<SupplierOrder | null>(null);
  let orderItems = $state<SupplierOrderItem[]>([]);

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m));
  onDestroy(() => scope.dispose());

  // Form fields
  let supplierName = $state('');
  let referenceNumber = $state('');
  let orderProducts = $state<{ product: Product; quantity: string }[]>([]);

  onMount(async () => {
    loadData();
  });

  /** تحديث القائمة دون تداخل مع ordersOp.run (تجنب الرفض عند busy) */
  async function refreshList() {
    const [nextOrders, nextProducts] = await Promise.all([
      listSupplierOrders(),
      listProducts(),
    ]);
    orders = nextOrders;
    products = nextProducts;
  }

  async function loadData() {
    await ordersOp.run(refreshList);
  }

  function openCreateModal() {
    editingOrderId = null;
    supplierName = '';
    referenceNumber = '';
    orderProducts = products.map((p) => ({ product: p, quantity: '' }));
    showModal = true;
    ordersOp.error.set(null);
  }

  async function openEditModal(order: SupplierOrder) {
    editingOrderId = order.id;
    supplierName = order.supplier_name;
    referenceNumber = order.reference_number || '';
    showModal = true;
    ordersOp.error.set(null);
    try {
      const items = await getSupplierOrderItems(order.id);
      const qtyByProduct = Object.fromEntries(
        items.map((i) => [i.product_id, String(i.quantity)])
      );
      orderProducts = products.map((p) => ({
        product: p,
        quantity: qtyByProduct[p.id] ?? '',
      }));
    } catch (e) {
      ordersOp.error.set(formatErrorMessage(e));
      closeModal();
    }
  }

  function closeModal() {
    showModal = false;
    editingOrderId = null;
    ordersOp.error.set(null);
  }

  async function saveOrder() {
    if (!supplierName) {
      ordersOp.error.set('الرجاء إدخال اسم المورد');
      return;
    }

    const items: OrderItemInput[] = orderProducts
      .filter((op) => op.quantity && parseFloat(op.quantity) > 0)
      .map((op) => ({
        product_id: op.product.id,
        quantity: parseFloat(op.quantity),
        unit_price: op.product.base_price,
      }));

    if (items.length === 0) {
      ordersOp.error.set('الرجاء إضافة منتج واحد على الأقل');
      return;
    }

    await ordersOp.run(async () => {
      if (editingOrderId) {
        await updateSupplierOrder({
          id: editingOrderId,
          supplier_name: supplierName,
          reference_number: referenceNumber || null,
          items,
        });
        setSuccessWithTimeout('تم تحديث الطلبية بنجاح');
      } else {
        await createSupplierOrder({
          supplier_name: supplierName,
          reference_number: referenceNumber || null,
          items,
        });
        setSuccessWithTimeout('تم إنشاء الطلبية بنجاح');
      }
      closeModal();
      await refreshList();
    });
  }

  async function handleDelete(order: SupplierOrder) {
    const yes = await showAsk(
      `هل أنت متأكد من حذف طلبية «${order.supplier_name}»؟ لا يمكن التراجع عن هذا الإجراء.`,
      { title: 'حذف الطلبية', kind: 'warning', okLabel: 'حذف', cancelLabel: 'إلغاء' }
    );
    if (!yes) return;

    await ordersOp.run(async () => {
      await deleteSupplierOrder(order.id);
      setSuccessWithTimeout('تم حذف الطلبية بنجاح');
      if (selectedOrder?.id === order.id) {
        closeDetails();
      }
      await refreshList();
    });
  }

  async function handleConfirm(order: SupplierOrder) {
    const yes = await showAsk('هل أنت متأكد من تأكيد هذه الطلبية؟ سيتم تحديث المخزون تلقائياً.', {
      title: 'تأكيد الطلبية',
      kind: 'warning'
    });
    if (!yes) return;

    await ordersOp.run(async () => {
      await confirmOrder(order.id);
      setSuccessWithTimeout('تم تأكيد الطلبية وتحديث المخزون');
      await refreshList();
    });
  }

  async function viewOrderDetails(order: SupplierOrder) {
    await ordersOp.run(async () => {
      selectedOrder = order;
      orderItems = await getSupplierOrderItems(order.id);
    });
  }

  function closeDetails() {
    selectedOrder = null;
    orderItems = [];
  }

  function getStatusIntent(status: string): 'success' | 'warning' | 'danger' | 'info' | 'neutral' {
    if (status === 'Confirmed') return 'success';
    if (status === 'Received') return 'info';
    if (status === 'Cancelled') return 'danger';
    if (status === 'Draft') return 'warning';
    return 'neutral';
  }

  function getStatusLabel(status: string): string {
    if (status === 'Draft') return 'مسودة';
    if (status === 'Confirmed') return 'مؤكدة';
    if (status === 'Received') return 'مستلمة';
    if (status === 'Cancelled') return 'ملغاة';
    return status;
  }
</script>

<Layout nodeType="UNIT" title="طلبيات الموردين" subtitle="إدارة الطلبيات والتوريد">
  <div class="mb-8" dir="rtl">
    <AppPageHeader title="طلبيات الموردين" subtitle="إدارة الطلبيات والتوريد">
      <svelte:fragment slot="actions">
        <AppButton variant="primary" on:click={openCreateModal}>
          <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
          </svg>
          طلبية جديدة
        </AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if $error && !showModal}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => ordersOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => success = ''}>{success}</AppAlert>
      </div>
    {/if}

    <AppCard padding="none">
      <AppTable
        loading={$loading}
        empty={!$loading && orders.length === 0}
      >
        <svelte:fragment slot="empty">
          <AppEmptyState
            title="لا يوجد طلبيات مسجلة"
            description="أنشئ أول طلبية للموردين"
            icon="M3 3h2l.4 2M7 13h10l4-8H5.4M7 13L5.4 5M7 13l-2.293 2.293c-.63.63-.184 1.707.707 1.707H17m0 0a2 2 0 100 4 2 2 0 000-4zm-8 2a2 2 0 11-4 0 2 2 0 014 0z"
          >
            <svelte:fragment slot="action">
              <AppButton variant="primary" on:click={openCreateModal}>إنشاء طلبيتك الأولى</AppButton>
            </svelte:fragment>
          </AppEmptyState>
        </svelte:fragment>

        <svelte:fragment slot="head">
          <th class="table-header">المورد</th>
          <th class="table-header">المرجع</th>
          <th class="table-header">التاريخ</th>
          <th class="table-header">المبلغ</th>
          <th class="table-header">الحالة</th>
          <th class="table-header text-left">الإجراءات</th>
        </svelte:fragment>

        {#each orders as order (order.id)}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors">
            <td class="table-cell font-medium">{order.supplier_name}</td>
            <td class="table-cell">{order.reference_number || '-'}</td>
            <td class="table-cell">{new Date(order.order_date).toLocaleDateString('fr-FR')}</td>
            <td class="table-cell">{order.total_amount?.toFixed(2) || '-'} DA</td>
            <td class="table-cell">
              <AppBadge intent={getStatusIntent(order.status)}>
                {getStatusLabel(order.status)}
              </AppBadge>
            </td>
            <td class="table-cell text-left space-x-2 space-x-reverse">
              <AppButton variant="ghost" size="sm" on:click={() => viewOrderDetails(order)} ariaLabel="عرض التفاصيل">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                </svg>
              </AppButton>
              {#if order.status === 'Draft'}
                <AppButton variant="ghost" size="sm" class="text-blue-600 hover:text-blue-800 dark:text-blue-400" on:click={() => openEditModal(order)} ariaLabel="تعديل الطلبية">
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/>
                  </svg>
                </AppButton>
                <AppButton variant="ghost" size="sm" class="text-green-600 hover:text-green-700 dark:text-green-500 dark:hover:text-green-400" on:click={() => handleConfirm(order)} ariaLabel="تأكيد (يحدث المخزون)">
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                  </svg>
                </AppButton>
                <AppButton variant="ghost" size="sm" class="text-red-600 hover:text-red-700 dark:text-red-400" on:click={() => handleDelete(order)} ariaLabel="حذف الطلبية">
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                  </svg>
                </AppButton>
              {/if}
            </td>
          </tr>
        {/each}
      </AppTable>
    </AppCard>
  </div>
</Layout>

<!-- Create Order Modal -->
<AppDialog open={showModal} title={editingOrderId ? 'تعديل الطلبية' : 'طلبية مورد جديدة'} size="xl" on:close={closeModal}>
  <div dir="rtl">
    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => ordersOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    <div class="grid grid-cols-2 gap-4 mb-6">
      <AppInput
        id="supplierName"
        label="المورد *"
        placeholder="اسم المورد"
        bind:value={supplierName}
      />
      <AppInput
        id="referenceNumber"
        label="المرجع"
        placeholder="رقم الفاتورة أو أمر الشراء"
        bind:value={referenceNumber}
      />
    </div>

    <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-4">المنتجات</h3>
    <div class="space-y-2 max-h-[40vh] overflow-y-auto pr-2">
      {#each orderProducts as op (op.product.id)}
        {@const qty = parseFloat(op.quantity) || 0}
        {@const lineTotal = qty * op.product.base_price}
        <div class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-lg">
          <span class="flex-1 font-medium text-sm text-gray-800 dark:text-gray-200">{op.product.name}</span>
          <div class="w-24">
            <AppInput
              id="qty-{op.product.id}"
              label=""
              type="number"
              placeholder="الكمية"
              bind:value={op.quantity}
            />
          </div>
          <div class="w-28 text-left text-sm text-gray-600 dark:text-gray-400 tabular-nums">
            <span class="block text-xs text-gray-500 dark:text-gray-500 mb-0.5">سعر الوحدة</span>
            <span class="font-medium">{op.product.base_price.toFixed(2)} دج</span>
          </div>
          <div class="w-28 text-left text-sm tabular-nums">
            <span class="block text-xs text-gray-500 dark:text-gray-500 mb-0.5">الإجمالي</span>
            <span class="font-medium text-gray-800 dark:text-gray-200">
              {lineTotal > 0 ? `${lineTotal.toFixed(2)} دج` : '—'}
            </span>
          </div>
        </div>
      {/each}
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="primary" on:click={saveOrder}>
      {editingOrderId ? 'حفظ التعديلات' : 'إنشاء الطلبية'}
    </AppButton>
  </svelte:fragment>
</AppDialog>

<!-- Order Details Modal -->
<AppDialog open={selectedOrder !== null} title="تفاصيل الطلبية" size="lg" on:close={closeDetails}>
  <div dir="rtl">
    {#if selectedOrder}
      <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">{selectedOrder.supplier_name} - {new Date(selectedOrder.order_date).toLocaleDateString('fr-FR')}</p>
      
      {#if orderItems.length === 0}
        <p class="text-gray-500 dark:text-gray-400">لا يوجد تفاصيل متاحة</p>
      {:else}
        <div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
          <table class="w-full text-sm">
            <thead class="bg-gray-50 dark:bg-gray-800/50">
              <tr>
                <th class="px-4 py-3 text-right font-medium text-gray-700 dark:text-gray-300">المنتج</th>
                <th class="px-4 py-3 text-left font-medium text-gray-700 dark:text-gray-300">الكمية</th>
                <th class="px-4 py-3 text-left font-medium text-gray-700 dark:text-gray-300">سعر الوحدة</th>
                <th class="px-4 py-3 text-left font-medium text-gray-700 dark:text-gray-300">الإجمالي</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
              {#each orderItems as item}
                <tr class="bg-white dark:bg-gray-900">
                  <td class="px-4 py-3">{item.product_name}</td>
                  <td class="px-4 py-3 text-left">{item.quantity.toFixed(2)}</td>
                  <td class="px-4 py-3 text-left">{item.unit_price.toFixed(2)} DA</td>
                  <td class="px-4 py-3 text-left font-medium">{item.total_cost.toFixed(2)} DA</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <div class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
          <div class="flex justify-between items-center">
            <span class="font-semibold text-gray-800 dark:text-gray-100">الإجمالي:</span>
            <span class="text-xl font-bold text-civil-blue">
              {orderItems.reduce((sum, item) => sum + item.total_cost, 0).toFixed(2)} دج
            </span>
          </div>
        </div>
      {/if}
    {/if}
  </div>
  
  <svelte:fragment slot="actions">
    {#if selectedOrder?.status === 'Draft'}
      <AppButton
        variant="secondary"
        on:click={() => {
          const order = selectedOrder;
          closeDetails();
          if (order) openEditModal(order);
        }}
      >
        تعديل
      </AppButton>
    {/if}
    <AppButton variant="secondary" on:click={closeDetails}>إغلاق</AppButton>
  </svelte:fragment>
</AppDialog>
