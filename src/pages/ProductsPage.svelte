<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatErrorMessage } from '../lib/errors';
  import {
    listProducts,
    createProduct,
    updateProduct,
    deleteProduct,
    exportProductsPackage,
    importProductsPackage,
    exportProductsExcel,
    getSettings,
    saveFile,
    openFile,
    showAsk
  } from '../lib/tauri';
  import type { Product, Settings, CreateProductRequest, UpdateProductRequest } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const productsOp = createOperation();
  const loading = productsOp.loading;
  const error = productsOp.error;

  let products: Product[] = [];
  let settings: Settings | null = null;
  let showModal = false;
  let editingProduct: Product | null = null;
  let success = '';
  let currentYear = new Date().getFullYear();

  let successTimeouts: number[] = [];
  function setSuccessWithTimeout(msg: string) {
    success = msg;
    const t = window.setTimeout(() => success = '', 3000);
    successTimeouts.push(t);
  }
  onDestroy(() => {
    successTimeouts.forEach(clearTimeout);
  });

  // Form fields
  let productName = '';
  let basePrice = '';
  let tva = '';
  let supplierName = '';

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    await productsOp.run(async () => {
      [products, settings] = await Promise.all([
        listProducts(),
        getSettings()
      ]);
      if (settings) {
        currentYear = settings.current_year;
      }
    });
  }

  function openCreateModal() {
    editingProduct = null;
    productName = '';
    basePrice = '';
    tva = '0';
    supplierName = '';
    showModal = true;
    productsOp.error.set(null);
  }

  function openEditModal(product: Product) {
    editingProduct = product;
    productName = product.name;
    basePrice = product.base_price.toString();
    tva = product.tva.toString();
    supplierName = product.supplier_name || '';
    showModal = true;
    productsOp.error.set(null);
  }

  function closeModal() {
    showModal = false;
    editingProduct = null;
    productsOp.error.set(null);
  }

  async function saveProduct() {
    if (!productName || !basePrice) {
      productsOp.error.set('Veuillez remplir tous les champs obligatoires');
      return;
    }

    await productsOp.run(async () => {
      const price = parseFloat(basePrice);
      const tvaValue = parseFloat(tva) || 0;

      if (editingProduct) {
        const request: UpdateProductRequest = {
          id: editingProduct.id,
          name: productName,
          base_price: price,
          tva: tvaValue,
          supplier_name: supplierName || null
        };
        await updateProduct(request);
        setSuccessWithTimeout('تم تحديث المنتج بنجاح');
      } else {
        const request: CreateProductRequest = {
          name: productName,
          base_price: price,
          tva: tvaValue,
          supplier_name: supplierName || null
        };
        await createProduct(request);
        setSuccessWithTimeout('تم إنشاء المنتج بنجاح');
      }

      closeModal();
      await loadData();
    });
  }

  async function handleDelete(product: Product) {
    const confirmed = await showAsk(`هل أنت متأكد من حذف المنتج "${product.name}" ؟`, {
      title: 'تأكيد الحذف',
      kind: 'warning',
      okLabel: 'نعم',
      cancelLabel: 'لا'
    });
    if (!confirmed) return;

    await productsOp.run(async () => {
      await deleteProduct(product.id);
      setSuccessWithTimeout('تم حذف المنتج بنجاح');
      await loadData();
    });
  }

  async function handleExport(format: 'excel' | 'package') {
    const extension = format === 'excel' ? 'xlsx' : 'sync';
    const filterName = format === 'excel' ? 'Excel' : 'حزمة المزامنة';
    
    const wilayaName = settings?.wilaya_name || 'الولاية';
    const defaultFilename = `منتجات_ولاية_${wilayaName}_${currentYear}.${extension}`;
    
    const filePath = await saveFile({
      filters: [{
        name: filterName,
        extensions: [extension]
      }],
      defaultPath: defaultFilename
    });

    if (!filePath) return;

    await productsOp.run(async () => {
      const result = format === 'excel'
          ? await exportProductsExcel(filePath)
          : await exportProductsPackage(filePath);
        
      if (result.success) {
        setSuccessWithTimeout(`تم التصدير بنجاح: ${result.record_count} منتجات (${filterName})`);
      }
    });
  }

  async function handleImport() {
    const extension = 'sync';
    const filterName = 'حزمة المزامنة';
    
    const selected = await openFile({
      multiple: false,
      filters: [{
        name: filterName,
        extensions: [extension]
      }]
    });

    if (!selected || Array.isArray(selected)) return;

    await productsOp.run(async () => {
      const result = await importProductsPackage(selected);
      const count = result.added;
        
      setSuccessWithTimeout(`تم استيراد ${count} منتجات بنجاح (${filterName})`);
      await loadData();
    });
  }
</script>

<Layout nodeType="WILAYA" title="إدارة المنتجات" subtitle="قائمة المنتجات والأسعار للسنة {currentYear}">
  <div dir="rtl" class="mb-8">
    <AppPageHeader title="إدارة المنتجات" subtitle="قائمة المنتجات والأسعار للسنة {currentYear}">
      <svelte:fragment slot="actions">
        <div class="flex items-center gap-2 flex-wrap">
          <AppButton
            variant="secondary"
            on:click={handleImport}
            ariaLabel="استيراد حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد"
          >
            <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
            </svg>
            استيراد
          </AppButton>
          
          <AppButton variant="secondary" on:click={() => handleExport('package')} ariaLabel="هذا هو مسار المزامنة الرسمي بين العقد">
            <svg class="w-4 h-4 mr-2 inline-block text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
            </svg>
            تصدير حزمة (.sync)
          </AppButton>

          <AppButton variant="secondary" on:click={() => handleExport('excel')}>
            <svg class="w-4 h-4 mr-2 inline-block text-green-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
            </svg>
            تصدير (Excel)
          </AppButton>

          <AppButton variant="primary" on:click={openCreateModal}>
            <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
            </svg>
            إضافة منتج
          </AppButton>
        </div>
      </svelte:fragment>
    </AppPageHeader>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => productsOp.error.set(null)}>{$error}</AppAlert>
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
        empty={!$loading && products.length === 0}
      >
        <svelte:fragment slot="empty">
          <AppEmptyState
            title="لا يوجد منتجات مسجلة لـ {currentYear}"
            description="أنشئ أول منتج أو قم بالاستيراد."
            icon="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
          >
            <svelte:fragment slot="action">
              <AppButton variant="primary" on:click={openCreateModal}>إنشاء أول منتج</AppButton>
            </svelte:fragment>
          </AppEmptyState>
        </svelte:fragment>

        <svelte:fragment slot="head">
          <th class="table-header">الاسم</th>
          <th class="table-header">السعر الأساسي</th>
          <th class="table-header">الضريبة %</th>
          <th class="table-header">المورد</th>
          <th class="table-header text-left">الإجراءات</th>
        </svelte:fragment>

        {#each products as product}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors">
            <td class="table-cell font-medium">{product.name}</td>
            <td class="table-cell">{product.base_price.toFixed(2)} دج</td>
            <td class="table-cell">{product.tva}%</td>
            <td class="table-cell">{product.supplier_name || '-'}</td>
            <td class="table-cell text-left space-x-2 space-x-reverse">
              <AppButton variant="ghost" size="sm" on:click={() => openEditModal(product)} ariaLabel="تعديل">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/>
                </svg>
              </AppButton>
              <AppButton variant="ghost" size="sm" class="text-gray-400 hover:text-red-500" on:click={() => handleDelete(product)} ariaLabel="حذف">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                </svg>
              </AppButton>
            </td>
          </tr>
        {/each}
      </AppTable>
    </AppCard>
  </div>
</Layout>

<!-- Modal -->
<AppDialog open={showModal} title={editingProduct ? 'تعديل المنتج' : 'منتج جديد'} on:close={closeModal}>
  <div dir="rtl">
    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => productsOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    <div class="space-y-4">
      <AppInput
        id="productName"
        label="الاسم *"
        placeholder="اسم المنتج"
        bind:value={productName}
      />

      <div class="grid grid-cols-2 gap-4">
        <AppInput
          id="basePrice"
          label="السعر الأساسي (دج) *"
          type="number"
          placeholder="0.00"
          bind:value={basePrice}
        />
        <AppInput
          id="tva"
          label="الضريبة (%)"
          type="number"
          placeholder="0"
          bind:value={tva}
        />
      </div>

      <AppInput
        id="supplierName"
        label="المورد"
        placeholder="اسم المورد (اختياري)"
        bind:value={supplierName}
      />
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="primary" on:click={saveProduct}>{editingProduct ? 'تحديث' : 'إنشاء'}</AppButton>
  </svelte:fragment>
</AppDialog>
