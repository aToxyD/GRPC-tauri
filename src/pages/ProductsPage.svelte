<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatErrorMessage } from '../lib/errors';
  import { saveFile, openFile, showAsk } from '../lib/tauri';
  import { exportProductsPackage, importProductsPackage } from '../lib/contracts';
  import { listProducts, createProduct, updateProduct, deleteProduct, getSettings, exportProductsExcel } from '../lib/contracts';
  import type { Product, Settings, CreateProductRequest, UpdateProductRequest } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  const productsOp = createOperation({ scope });
  const loading = productsOp.loading;
  const error = productsOp.error;

  // @category ProjectionState
  let products = $state<Product[]>([]);
  // @category ProjectionState
  let settings = $state<Settings | null>(null);
  // @category UiState
  let showModal = $state(false);
  // @category UiState
  let editingProduct = $state<Product | null>(null);
  // @category TransientState
  let success = $state('');
  // @category UiState
  let currentYear = $state(new Date().getFullYear());

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m));
  onDestroy(() => scope.dispose());

  // Form fields
  // @category TransientState
  let productName = $state('');
  // @category TransientState
  let basePrice = $state('');
  // @category TransientState
  let purchaseUnit = $state<number | ''>('');
  // @category TransientState
  let consumptionUnit = $state<number | ''>('');
  // @category TransientState
  let conversionFactor = $state<string>('1');
  // @category TransientState
  let tvaClassification = $state<number | ''>('');

  // SEC-087 Phase 6D: presentation-only label maps for the backend unit codes
  // (UnitMeasure 1..=10) and TVA classification codes (TvaClassification
  // 0..=2). Display only — canonical codes are the wire contract (A5/F3).
  const UNIT_LABELS: Record<number, string> = {
    1: 'كلغ', 2: 'لتر', 3: 'دلو', 4: 'قارورة', 5: 'صفيحة',
    6: 'قطعة', 7: 'بيضة', 8: 'علبة', 9: 'كيس', 10: 'خبزة',
  };
  const UNIT_CODES = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
  const TVA_OPTIONS: { code: number; label: string }[] = [
    { code: 0, label: 'EXONÉRÉ' },
    { code: 1, label: '9 %' },
    { code: 2, label: '19 %' },
  ];
  const TVA_LABELS: Record<number, string> = {
    0: 'EXONÉRÉ',
    1: '9 %',
    2: '19 %',
  };

  function unitLabel(code: number): string {
    return UNIT_LABELS[code] ?? `${code}`;
  }

  // Same-unit config forces the conversion factor to 1 (domain rule).
  // @category DerivedState
  const unitsEqual = $derived(
    purchaseUnit !== '' && consumptionUnit !== '' && purchaseUnit === consumptionUnit,
  );

  function syncConversionFactor() {
    if (unitsEqual) {
      conversionFactor = '1';
    }
  }

  onMount(async () => {
    await loadData();
  });

  /** تحديث القائمة دون تداخل مع productsOp.run (تجنب الرفض عند busy) */
  async function refreshList() {
    const [nextProducts, nextSettings] = await Promise.all([listProducts(), getSettings()]);
    products = nextProducts;
    settings = nextSettings;
    if (nextSettings) {
      currentYear = nextSettings.current_year;
    }
  }

  async function loadData() {
    await productsOp.run(refreshList);
  }

  function openCreateModal() {
    editingProduct = null;
    productName = '';
    basePrice = '';
    purchaseUnit = '';
    consumptionUnit = '';
    conversionFactor = '1';
    tvaClassification = '';
    showModal = true;
    productsOp.error.set(null);
  }

  function openEditModal(product: Product) {
    editingProduct = product;
    productName = product.name;
    basePrice = product.base_price.toString();
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

    const price = parseFloat(basePrice);
    if (!editingProduct) {
      // SEC-087: unit/TVA config is mandatory for newly created products.
      if (purchaseUnit === '' || consumptionUnit === '' || tvaClassification === '') {
        productsOp.error.set('Veuillez sélectionner les unités et la classification TVA');
        return;
      }
      const factor = parseInt(conversionFactor, 10);
      if (!Number.isInteger(factor) || factor <= 0) {
        productsOp.error.set('Le facteur de conversion doit être un entier supérieur à 0');
        return;
      }
      if (purchaseUnit === consumptionUnit && factor !== 1) {
        productsOp.error.set('Le facteur de conversion doit être 1 lorsque les unités sont identiques');
        return;
      }
    }

    await productsOp.run(async () => {
      if (editingProduct) {
        const request: UpdateProductRequest = {
          id: editingProduct.id,
          name: productName,
          base_price: price,
        };
        await updateProduct(request);
        setSuccessWithTimeout('تم تحديث المنتج بنجاح');
      } else {
        const request: CreateProductRequest = {
          name: productName,
          base_price: price,
          purchase_unit: Number(purchaseUnit),
          consumption_unit: Number(consumptionUnit),
          conversion_factor: parseInt(conversionFactor, 10),
          tva_classification: Number(tvaClassification),
        };
        await createProduct(request);
        setSuccessWithTimeout('تم إنشاء المنتج بنجاح');
      }

      closeModal();
      await refreshList();
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
      await refreshList();
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
      await refreshList();
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
          <th class="table-header">السعر المرجعي</th>
          <th class="table-header">الوحدات</th>
          <th class="table-header">TVA</th>
          <th class="table-header text-left">الإجراءات</th>
        </svelte:fragment>

        {#each products as product (product.id)}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors">
            <td class="table-cell font-medium">{product.name}</td>
            <td class="table-cell">{product.base_price.toFixed(2)} دج</td>
            <td class="table-cell text-xs">
              {unitLabel(product.purchase_unit)} → {unitLabel(product.consumption_unit)}
              {#if product.conversion_factor !== 1}×{product.conversion_factor}{/if}
            </td>
            <td class="table-cell text-xs">{TVA_LABELS[product.tva_classification] ?? product.tva_classification}</td>
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

      <AppInput
        id="basePrice"
        label="السعر المرجعي (دج) *"
        type="number"
        placeholder="0.00"
        bind:value={basePrice}
      />
      <AppAlert intent="info">
        السعر المرجعي أساس قياسي فقط ولا يمثل سعر الشراء. أسعار الشراء الفعلية محددة في عقود التموين (agreed price) ضمن كتالوج العقود.
      </AppAlert>

      {#if !editingProduct}
        <AppSelect
          id="purchaseUnit"
          label="وحدة الشراء *"
          bind:value={purchaseUnit}
          required
          on:change={syncConversionFactor}
        >
          <option value="">-- اختر وحدة الشراء --</option>
          {#each UNIT_CODES as code}
            <option value={code}>{unitLabel(code)}</option>
          {/each}
        </AppSelect>

        <AppSelect
          id="consumptionUnit"
          label="وحدة الاستهلاك *"
          bind:value={consumptionUnit}
          required
          on:change={syncConversionFactor}
        >
          <option value="">-- اختر وحدة الاستهلاك --</option>
          {#each UNIT_CODES as code}
            <option value={code}>{unitLabel(code)}</option>
          {/each}
        </AppSelect>

        <AppInput
          id="conversionFactor"
          label="معامل التحويل *"
          type="number"
          min="1"
          placeholder="1"
          disabled={unitsEqual}
          bind:value={conversionFactor}
        />
        {#if unitsEqual}
          <AppAlert intent="info">
            عندما تتطابق وحدة الشراء ووحدة الاستهلاك، يكون معامل التحويل 1.
          </AppAlert>
        {/if}

        <AppSelect id="tvaClassification" label="تصنيف TVA *" bind:value={tvaClassification} required>
          <option value="">-- اختر تصنيف TVA --</option>
          {#each TVA_OPTIONS as opt}
            <option value={opt.code}>{opt.label}</option>
          {/each}
        </AppSelect>
      {/if}
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="primary" on:click={saveProduct}>{editingProduct ? 'تحديث' : 'إنشاء'}</AppButton>
  </svelte:fragment>
</AppDialog>
