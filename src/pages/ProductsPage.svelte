<script lang="ts">
  import { onMount } from 'svelte';
  import { listProducts, createProduct, updateProduct, deleteProduct, exportProductsPackage, importProductsPackage, exportProductsExcel, getSettings } from '../lib/tauri';
  import { save } from '@tauri-apps/plugin-dialog';
  import { open } from '@tauri-apps/plugin-dialog';
  import type { Product, Settings, CreateProductRequest, UpdateProductRequest } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let products: Product[] = [];
  let settings: Settings | null = null;
  let loading = true;
  let showModal = false;
  let editingProduct: Product | null = null;
  let error = '';
  let success = '';
  let currentYear = new Date().getFullYear();
  let showExportDropdown = false;
  let showImportDropdown = false;

  // Form fields
  let productName = '';
  let basePrice = '';
  let tva = '';
  let supplierName = '';

  onMount(async () => {
    loadData();
  });

  async function loadData() {
    try {
      loading = true;
      [products, settings] = await Promise.all([
        listProducts(),
        getSettings()
      ]);
      if (settings) {
        currentYear = settings.current_year;
      }
    } catch (e) {
      error = 'خطأ في التحميل: ' + String(e);
    } finally {
      loading = false;
    }
  }

  function openCreateModal() {
    editingProduct = null;
    productName = '';
    basePrice = '';
    tva = '0';
    supplierName = '';
    showModal = true;
    error = '';
  }

  function openEditModal(product: Product) {
    editingProduct = product;
    productName = product.name;
    basePrice = product.base_price.toString();
    tva = product.tva.toString();
    supplierName = product.supplier_name || '';
    showModal = true;
    error = '';
  }

  function closeModal() {
    showModal = false;
    editingProduct = null;
    error = '';
  }

  async function saveProduct() {
    if (!productName || !basePrice) {
      error = 'Veuillez remplir tous les champs obligatoires';
      return;
    }

    try {
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
        success = 'تم تحديث المنتج بنجاح';
      } else {
        const request: CreateProductRequest = {
          name: productName,
          base_price: price,
          tva: tvaValue,
          supplier_name: supplierName || null
        };
        await createProduct(request);
        success = 'تم إنشاء المنتج بنجاح';
      }

      closeModal();
      loadData();
      setTimeout(() => success = '', 3000);
    } catch (e) {
      error = 'خطأ: ' + String(e);
    }
  }

  async function handleDelete(product: Product) {
    if (!confirm(`هل أنت متأكد من حذف المنتج "${product.name}" ؟`)) {
      return;
    }

    try {
      await deleteProduct(product.id);
      success = 'تم حذف المنتج بنجاح';
      loadData();
      setTimeout(() => success = '', 3000);
    } catch (e) {
      error = 'خطأ في الحذف: ' + String(e);
    }
  }

  async function handleExport(format: 'csv' | 'excel' | 'package') {
    try {
      const extension = format === 'csv' ? 'csv' : format === 'excel' ? 'xlsx' : 'sync';
      const filterName = format === 'csv' ? 'CSV' : format === 'excel' ? 'Excel' : 'حزمة المزامنة';
      
      const wilayaName = settings?.wilaya_name || 'الولاية';
      const defaultFilename = `منتجات_ولاية_${wilayaName}_${currentYear}.${extension}`;
      
      const filePath = await save({
        filters: [{
          name: filterName,
          extensions: [extension]
        }],
        defaultPath: defaultFilename
      });

      if (filePath) {
        const result = format === 'excel'
            ? await exportProductsExcel(filePath)
            : await exportProductsPackage(filePath);
          
        if (result.success) {
          success = `تم التصدير بنجاح: ${result.record_count} منتجات (${filterName})`;
          setTimeout(() => success = '', 3000);
        }
      }
      showExportDropdown = false;
    } catch (e) {
      error = 'خطأ في التصدير: ' + String(e);
      showExportDropdown = false;
    }
  }

  async function handleImport() {
    try {
      const extension = 'sync';
      const filterName = 'حزمة المزامنة';
      
      const selected = await open({
        multiple: false,
        filters: [{
          name: filterName,
          extensions: [extension]
        }]
      });

      if (selected) {
        const result = await importProductsPackage(selected as string);
        const count = result.added;
          
        success = `تم استيراد ${count} منتجات بنجاح (${filterName})`;
        loadData();
        setTimeout(() => success = '', 3000);
      }
      showImportDropdown = false;
    } catch (e) {
      error = 'خطأ في الاستيراد: ' + String(e);
      showImportDropdown = false;
    }
  }
</script>

<Layout nodeType="WILAYA" title="إدارة المنتجات" subtitle="قائمة المنتجات والأسعار للسنة {currentYear}">
    <div class="mb-8">
      <div class="flex items-center justify-end gap-3">
          <!-- Import Dropdown -->
            <button
              on:click={handleImport}
              class="btn-secondary flex items-center gap-2"
              title="استيراد حزمة المزامنة (.sync) - هذا هو مسار المزامنة الرسمي بين العقد"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
              </svg>
              <span>استيراد</span>
            </button>

          <!-- Export Dropdown -->
          <div class="relative">
            <button
              on:click={() => showExportDropdown = !showExportDropdown}
              class="btn-secondary flex items-center gap-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
              </svg>
              <span>تصدير</span>
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
              </svg>
            </button>
            
            {#if showExportDropdown}
              <div class="absolute top-full left-0 mt-1 w-48 bg-white rounded-lg shadow-lg border border-gray-200 z-10">
                <button
                  on:click={() => handleExport('package')}
                  class="w-full text-right px-4 py-2 hover:bg-gray-50 flex items-center justify-between"
                  title="هذا هو مسار المزامنة الرسمي بين العقد"
                >
                  <span>حزمة المزامنة</span>
                  <span class="text-xs text-gray-500">.sync</span>
                </button>
                <button
                  on:click={() => handleExport('excel')}
                  class="w-full text-right px-4 py-2 hover:bg-gray-50 flex items-center justify-between"
                >
                  <span>Excel</span>
                  <span class="text-xs text-gray-500">.xlsx</span>
                </button>
              </div>
            {/if}
          </div>

          <button
            on:click={openCreateModal}
            class="btn-primary flex items-center gap-2"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
            </svg>
            <span>إضافة منتج</span>
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
      {:else if products.length === 0}
        <div class="text-center py-12 text-gray-500">
          <svg class="w-16 h-16 mx-auto mb-4 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/>
          </svg>
          <p class="text-lg">لا يوجد منتجات مسجلة لـ {currentYear}</p>
          <button on:click={openCreateModal} class="text-civil-blue hover:underline mt-2">
            إنشاء أول منتج
          </button>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr>
                <th class="table-header">الاسم</th>
                <th class="table-header">السعر الأساسي</th>
                <th class="table-header">الضريبة %</th>
                <th class="table-header">المورد</th>
                <th class="table-header text-right">الإجراءات</th>
              </tr>
            </thead>
            <tbody>
              {#each products as product}
                <tr class="hover:bg-gray-50">
                  <td class="table-cell font-medium">{product.name}</td>
                  <td class="table-cell">{product.base_price.toFixed(2)} دج</td>
                  <td class="table-cell">{product.tva}%</td>
                  <td class="table-cell">{product.supplier_name || '-'}</td>
                  <td class="table-cell text-right">
                    <button
                      on:click={() => openEditModal(product)}
                      class="text-civil-blue hover:text-civil-blue-dark mr-3"
                      title="تعديل"
                    >
                      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/>
                      </svg>
                    </button>
                    <button
                      on:click={() => handleDelete(product)}
                      class="text-gray-400 hover:text-red-500"
                      title="حذف"
                    >
                      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                      </svg>
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

<!-- Modal -->
{#if showModal}
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl w-full max-w-md mx-4">
      <div class="p-6 border-b border-gray-100">
        <h2 class="text-xl font-semibold text-gray-800">
          {editingProduct ? 'تعديل المنتج' : 'منتج جديد'}
        </h2>
      </div>

      <div class="p-6 space-y-4">
        {#if error}
          <div class="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
            {error}
          </div>
        {/if}

        <div>
          <label for="productName" class="block text-sm font-medium text-gray-700 mb-1">الاسم *</label>
          <input
            id="productName"
            type="text"
            class="input-field"
            placeholder="اسم المنتج"
            bind:value={productName}
          />
        </div>

        <div class="grid grid-cols-2 gap-4">
          <div>
            <label for="basePrice" class="block text-sm font-medium text-gray-700 mb-1">السعر الأساسي (دج) *</label>
            <input
              id="basePrice"
              type="number"
              step="0.01"
              class="input-field"
              placeholder="0.00"
              bind:value={basePrice}
            />
          </div>
          <div>
            <label for="tva" class="block text-sm font-medium text-gray-700 mb-1">الضريبة (%)</label>
            <input
              id="tva"
              type="number"
              step="0.01"
              class="input-field"
              placeholder="0"
              bind:value={tva}
            />
          </div>
        </div>

        <div>
          <label for="supplierName" class="block text-sm font-medium text-gray-700 mb-1">المورد</label>
          <input
            id="supplierName"
            type="text"
            class="input-field"
            placeholder="اسم المورد (اختياري)"
            bind:value={supplierName}
          />
        </div>
      </div>

      <div class="p-6 border-t border-gray-100 flex justify-end gap-3">
        <button
          on:click={closeModal}
          class="btn-secondary"
        >
          إلغاء
        </button>
        <button
          on:click={saveProduct}
          class="btn-primary"
        >
          {editingProduct ? 'تحديث' : 'إنشاء'}
        </button>
      </div>
    </div>
  </div>
{/if}
</Layout>
