<script lang="ts">
  import { onMount } from 'svelte';
  import { listSupplierOrders, createSupplierOrder, confirmOrder, listProducts, getSupplierOrderItems } from '../lib/tauri';
  import type { SupplierOrder, Product, OrderItemInput, SupplierOrderItem } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let orders: SupplierOrder[] = [];
  let products: Product[] = [];
  let loading = true;
  let showModal = false;
  let error = '';
  let success = '';
  let selectedOrder: SupplierOrder | null = null;
  let orderItems: SupplierOrderItem[] = [];

  // Form fields
  let supplierName = '';
  let referenceNumber = '';
  let orderProducts: { product: Product; quantity: string; unitPrice: string }[] = [];

  onMount(async () => {
    loadData();
  });

  async function loadData() {
    try {
      loading = true;
      [orders, products] = await Promise.all([
        listSupplierOrders(),
        listProducts()
      ]);
    } catch (e) {
      error = 'خطأ في التحميل: ' + String(e);
    } finally {
      loading = false;
    }
  }

  function openCreateModal() {
    supplierName = '';
    referenceNumber = '';
    orderProducts = products.map(p => ({ product: p, quantity: '', unitPrice: p.base_price.toString() }));
    showModal = true;
    error = '';
  }

  function closeModal() {
    showModal = false;
    selectedOrder = null;
    orderItems = [];
    error = '';
  }

  async function saveOrder() {
    if (!supplierName) {
      error = 'الرجاء إدخال اسم المورد';
      return;
    }

    const items: OrderItemInput[] = orderProducts
      .filter(op => op.quantity && parseFloat(op.quantity) > 0)
      .map(op => ({
        product_id: op.product.id,
        quantity: parseFloat(op.quantity),
        unit_price: parseFloat(op.unitPrice) || op.product.base_price
      }));

    if (items.length === 0) {
      error = 'الرجاء إضافة منتج واحد على الأقل';
      return;
    }

    try {
      await createSupplierOrder({
        supplier_name: supplierName,
        reference_number: referenceNumber || null,
        items
      });
      success = 'تم إنشاء الطلبية بنجاح';
      closeModal();
      loadData();
      setTimeout(() => success = '', 3000);
    } catch (e) {
      error = 'خطأ: ' + String(e);
    }
  }

  async function handleConfirm(order: SupplierOrder) {
    if (!confirm('هل أنت متأكد من تأكيد هذه الطلبية؟ سيتم تحديث المخزون تلقائياً.')) {
      return;
    }

    try {
      await confirmOrder(order.id);
      success = 'تم تأكيد الطلبية وتحديث المخزون';
      loadData();
      setTimeout(() => success = '', 3000);
    } catch (e) {
      error = 'خطأ: ' + String(e);
    }
  }

  async function viewOrderDetails(order: SupplierOrder) {
    try {
      selectedOrder = order;
      orderItems = await getSupplierOrderItems(order.id);
    } catch (e) {
      error = 'خطأ في تحميل التفاصيل: ' + String(e);
    }
  }

  function closeDetails() {
    selectedOrder = null;
    orderItems = [];
  }
</script>

<Layout nodeType="UNIT" title="طلبيات الموردين" subtitle="إدارة الطلبيات والتوريد">
  <div class="mb-8">
    <div class="flex items-center justify-end">
      <button
        on:click={openCreateModal}
        class="btn-primary flex items-center gap-2"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
        </svg>
        <span>طلبية جديدة</span>
      </button>
    </div>
  </div>

    {#if error}
      <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
        {error}
      </div>
    {/if}

    {#if success}
      <div class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm">
        {success}
      </div>
    {/if}

    <div class="card">
      {#if loading}
        <div class="flex items-center justify-center py-12">
          <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"></div>
        </div>
      {:else if orders.length === 0}
        <div class="text-center py-12 text-gray-500 dark:text-gray-400">
          <svg class="w-16 h-16 mx-auto mb-4 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 3h2l.4 2M7 13h10l4-8H5.4M7 13L5.4 5M7 13l-2.293 2.293c-.63.63-.184 1.707.707 1.707H17m0 0a2 2 0 100 4 2 2 0 000-4zm-8 2a2 2 0 11-4 0 2 2 0 014 0z"/>
          </svg>
          <p class="text-lg">لا يوجد طلبيات مسجلة</p>
          <button on:click={openCreateModal} class="text-civil-blue hover:underline mt-2">
            إنشاء طلبيتك الأولى
          </button>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">المورد</th>
                <th class="table-header">المرجع</th>
                <th class="table-header">التاريخ</th>
                <th class="table-header">المبلغ</th>
                <th class="table-header">الحالة</th>
                <th class="table-header text-right">الإجراءات</th>
              </tr>
            </thead>
            <tbody>
              {#each orders as order}
                <tr class="hover:bg-gray-50 dark:bg-gray-900">
                  <td class="table-cell font-medium">{order.supplier_name}</td>
                  <td class="table-cell">{order.reference_number || '-'}</td>
                  <td class="table-cell">{new Date(order.order_date).toLocaleDateString('fr-FR')}</td>
                  <td class="table-cell">{order.total_amount?.toFixed(2) || '-'} DA</td>
                  <td class="table-cell">
                    <span class="px-2 py-1 rounded-full text-xs font-medium
                      {order.status === 'Confirmed' ? 'bg-green-100 text-green-800' : 
                       order.status === 'Received' ? 'bg-blue-100 text-blue-800 dark:text-blue-300' :
                       order.status === 'Cancelled' ? 'bg-red-100 text-red-800' :
                       'bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-100'}">
                      {order.status === 'Draft' ? 'مسودة' :
                       order.status === 'Confirmed' ? 'مؤكدة' :
                       order.status === 'Received' ? 'مستلمة' : 'ملغاة'}
                    </span>
                  </td>
                  <td class="table-cell text-right">
                    <button
                      on:click={() => viewOrderDetails(order)}
                      class="text-civil-blue hover:text-civil-blue-dark mr-3"
                      title="عرض التفاصيل"
                    >
                      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/>
                      </svg>
                    </button>
                    {#if order.status === 'Draft'}
                      <button
                        on:click={() => handleConfirm(order)}
                        class="text-green-600 hover:text-green-700"
                        title="تأكيد (يحدث المخزون)"
                      >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                        </svg>
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
</Layout>

<!-- Create Order Modal -->
{#if showModal}
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-4xl mx-4 max-h-[90vh] overflow-hidden">
      <div class="p-6 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">طلبية مورد جديدة</h2>
        <button on:click={closeModal} class="text-gray-400 hover:text-gray-600 dark:text-gray-400" aria-label="إغلاق">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
          </svg>
        </button>
      </div>

      <div class="p-6 overflow-y-auto max-h-[60vh]">
        {#if error}
          <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
            {error}
          </div>
        {/if}

        <div class="grid grid-cols-2 gap-4 mb-6">
          <div>
            <label for="supplierName" class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1">المورد *</label>
            <input
              id="supplierName"
              type="text"
              class="input-field"
              placeholder="اسم المورد"
              bind:value={supplierName}
            />
          </div>
          <div>
            <label for="referenceNumber" class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1">المرجع</label>
            <input
              id="referenceNumber"
              type="text"
              class="input-field"
              placeholder="رقم الفاتورة أو أمر الشراء"
              bind:value={referenceNumber}
            />
          </div>
        </div>

        <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-4">المنتجات</h3>
        <div class="space-y-2">
          {#each orderProducts as op}
            <div class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
              <span class="flex-1 font-medium">{op.product.name}</span>
              <span class="text-sm text-gray-500 dark:text-gray-400">السعر: {op.product.base_price.toFixed(2)} دج</span>
              <input
                type="number"
                step="0.01"
                class="input-field w-24"
                placeholder="الكمية"
                bind:value={op.quantity}
              />
              <input
                type="number"
                step="0.01"
                class="input-field w-28"
                placeholder="سعر الوحدة"
                bind:value={op.unitPrice}
              />
            </div>
          {/each}
        </div>
      </div>

      <div class="p-6 border-t border-gray-100 dark:border-gray-700 flex justify-end gap-3">
        <button on:click={closeModal} class="btn-secondary">إلغاء</button>
        <button on:click={saveOrder} class="btn-primary">إنشاء الطلبية</button>
      </div>
    </div>
  </div>
{/if}

<!-- Order Details Modal -->
{#if selectedOrder}
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-2xl mx-4">
      <div class="p-6 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
        <div>
          <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">تفاصيل الطلبية</h2>
          <p class="text-sm text-gray-500 dark:text-gray-400">{selectedOrder.supplier_name} - {new Date(selectedOrder.order_date).toLocaleDateString('fr-FR')}</p>
        </div>
        <button on:click={closeDetails} class="text-gray-400 hover:text-gray-600 dark:text-gray-400" aria-label="إغلاق">
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
          </svg>
        </button>
      </div>

      <div class="p-6">
        {#if orderItems.length === 0}
          <p class="text-gray-500 dark:text-gray-400">لا يوجد تفاصيل متاحة</p>
        {:else}
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">المنتج</th>
                <th class="table-header text-right">الكمية</th>
                <th class="table-header text-right">سعر الوحدة</th>
                <th class="table-header text-right">الإجمالي</th>
              </tr>
            </thead>
            <tbody>
              {#each orderItems as item}
                <tr>
                  <td class="table-cell">{item.product_name}</td>
                  <td class="table-cell text-right">{item.quantity.toFixed(2)}</td>
                  <td class="table-cell text-right">{item.unit_price.toFixed(2)} DA</td>
                  <td class="table-cell text-right font-medium">{item.total_cost.toFixed(2)} DA</td>
                </tr>
              {/each}
            </tbody>
          </table>

          <div class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
            <div class="flex justify-between items-center">
              <span class="font-semibold text-gray-800 dark:text-gray-100">الإجمالي:</span>
              <span class="text-xl font-bold text-civil-blue">
                {orderItems.reduce((sum, item) => sum + item.total_cost, 0).toFixed(2)} دج
              </span>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
