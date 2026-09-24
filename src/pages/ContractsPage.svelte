<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { saveFile, showAsk } from '../lib/tauri';
  import {
    listContracts,
    listUnitSuppliers,
    getContractProducts,
    listContractAllocations,
    listAllocationExceptions,
    createContract,
    addContractProduct,
    setContractProductAgreedPriceHt,
    acceptContract,
    activateContract,
    endContract,
    cancelContract,
    releaseContractAllocation,
    revokeContractAllocationRelease,
    exportContractsExcel,
    exportContractAllocationsExcel,
    exportContractCatalogToUnits,
    listUnits,
    listSuppliers,
    listProducts,
    calculateProductPriceWithTva,
    getSettings,
  } from '../lib/contracts';
  import type {
    Supplier, Unit, Product, Settings, Contract, ContractProduct,
    ContractAllocation, ContractAllocationException, CreateContractRequest,
    AddContractProductRequest, ReleaseContractAllocationRequest, ReleaseReasonCode,
  } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';
  import { unitLabel } from '../lib/unitLabels';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppProductSearch from '../lib/components/ui/AppProductSearch.svelte';

  const scope = createRuntimeScope();
  const contractsOp = createOperation({ scope });
  const loading = contractsOp.loading;
  const error = contractsOp.error;

  // @category ProjectionState
  let contracts = $state<Contract[]>([]);
  // @category ProjectionState
  let units = $state<Unit[]>([]);
  // @category ProjectionState
  let suppliers = $state<Supplier[]>([]);
  // @category ProjectionState
  let products = $state<Product[]>([]);
  // @category ProjectionState
  let settings = $state<Settings | null>(null);
  // @category ProjectionState
  let unitSuppliers = $state<Supplier[]>([]);
  // @category ProjectionState
  let selectedContract = $state<Contract | null>(null);
  // @category ProjectionState
  let selectedProducts = $state<ContractProduct[]>([]);
  // @category ProjectionState
  let selectedAllocations = $state<ContractAllocation[]>([]);
  // @category ProjectionState
  let allocationExceptions = $state<Record<string, ContractAllocationException[]>>({});
  // @category UiState
  let contractProductsSearch = $state('');
  // @category UiState
  let allocationsSearch = $state('');
  // @category TransientState
  let success = $state('');
  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m));
  onDestroy(() => scope.dispose());

  // Create-contract form
  // @category TransientState
  let showCreateModal = $state(false);
  // @category TransientState
  let newUnitId = $state('');
  // @category TransientState
  let newSupplierId = $state('');
  // @category TransientState
  let newFiscalYear = $state<number>(new Date().getFullYear());
  // @category TransientState
  let newReference = $state('');
  // @category TransientState
  let newNotes = $state('');

  // ContractCatalog UNIT distribution (ADR-0059 / SEC-087-F)
  // @category TransientState
  let showDistributeModal = $state(false);
  // @category TransientState
  let distributeSelectedUnitIds = $state<string[]>([]);

  // Add-product form
  // @category TransientState
  let showAddProductModal = $state(false);
  // @category TransientState
  let newProductId = $state('');
  // @category TransientState
  let newProposedPrice = $state('');
  // @category TransientState
  let newAgreedPrice = $state('');
  // @category TransientState
  let newQuantity = $state('');

  // Set-agreed-price form
  // @category TransientState
  let agreedPriceTarget = $state<ContractProduct | null>(null);

  // @category DerivedState
  let addProductTvaRate = $state<number | null>(null);
  // @category DerivedState
  let addProductTtcPreview = $state('');
  // @category DerivedState
  let agreedPriceTvaRate = $state<number | null>(null);
  // @category DerivedState
  let agreedPriceTtcRecap = $state('');

  // TVA rate (%) of a catalog product used for a read-only display/preview. Never a business rule.
  function catalogTvaRateById(productId: string): number | null {
    return products.find((p) => p.id === productId)?.tva_rate ?? null;
  }

  // Presentation-only organizational UNIT label: `name (code)`, falling back to the raw id.
  function orgUnitLabel(unitId: string): string {
    const unit = units.find((u) => u.id === unitId);
    return unit ? `${unit.name} (${unit.code})` : unitId;
  }

  // Presentation-only matched allocation quantity for a ContractProduct row.
  // The value is read verbatim from the authoritative `ContractAllocation`
  // projection (`contracted_quantity`); it is never re-derived here.
  // @category UiState
  function contractedQtyForProduct(contractProductId: string): number | null {
    return selectedAllocations.find(
      (a) => a.contract_product_id === contractProductId
    )?.contracted_quantity ?? null;
  }

  // Task 1c/1f: the proposed price is read-only and always snapshots the WILAYA
  // catalog ReferencePrice (`Product.base_price`) of the selected Product, so
  // the operator compares it against the agreed price instead of typing it.
  // @category UiState
  function orderQuantityLabel(): string {
    const selected = products.find((p) => p.id === newProductId);
    return selected
      ? `الكمية المتفق عليها * (${unitLabel(selected.purchase_unit)})`
      : 'الكمية المتفق عليها *';
  }

  // Presentation-only product name of an obligation/entitlement row, resolved
  // from the WILAYA catalog. Never a business decision.
  // @category UiState
  function allocationProductName(allocation: ContractAllocation): string {
    return products.find((p) => p.id === allocation.product_id)?.name || allocation.product_id;
  }

  // Search is presentation/filtering only (first contract list): it narrows the
  // products of the currently displayed contract and never injects unrelated
  // WILAYA catalog products into the contract list.
  // @category UiState
  let filteredSelectedProducts = $derived(
    contractProductsSearch.trim()
      ? selectedProducts.filter((p) =>
          p.product_name.toLowerCase().includes(contractProductsSearch.trim().toLowerCase())
        )
      : selectedProducts
  );

  // Search is presentation/filtering only (unit entitlements/obligations list):
  // it narrows the existing obligation rows by product name and never alters
  // entitlement calculations or contract semantics.
  // @category UiState
  let filteredSelectedAllocations = $derived(
    allocationsSearch.trim()
      ? selectedAllocations.filter((a) =>
          allocationProductName(a).toLowerCase().includes(allocationsSearch.trim().toLowerCase())
        )
      : selectedAllocations
  );

  // Task 1d: the add-product TTC preview depends ONLY on the agreed HT price
  // and the selected Product's authoritative TVA rate. The proposed price is
  // never used in this calculation path — TTC is backend-derived by
  // `calculate_product_price_with_tva` and this preview is non-binding.
  // @category Effect
  $effect(() => {
    const selected = products.find((p) => p.id === newProductId);
    newProposedPrice = selected ? String(selected.base_price) : '';
    const rate = selected?.tva_rate ?? null;
    addProductTvaRate = rate;
    const price = parseFloat(newAgreedPrice);
    if (newProductId === '' || Number.isNaN(price) || price < 0 || rate === null) {
      addProductTtcPreview = '';
      return;
    }
    calculateProductPriceWithTva(price, rate)
      .then((ttc) => {
        addProductTtcPreview = ttc.toFixed(2);
      })
      .catch(() => {
        addProductTtcPreview = '';
      });
  });

  // @category Effect
  $effect(() => {
    const t = agreedPriceTarget;
    if (t === null) {
      agreedPriceTvaRate = null;
      agreedPriceTtcRecap = '';
      return;
    }
    const rate = t.tva_rate ?? catalogTvaRateById(t.product_id);
    agreedPriceTvaRate = rate;
    const price = parseFloat(newAgreedPrice);
    if (rate === null || Number.isNaN(price) || price < 0) {
      agreedPriceTtcRecap = '';
      return;
    }
    calculateProductPriceWithTva(price, rate)
      .then((ttc) => {
        agreedPriceTtcRecap = ttc.toFixed(2);
      })
      .catch(() => {
        agreedPriceTtcRecap = '';
      });
  });

  // Release form
  // @category TransientState
  let releaseTarget = $state<ContractAllocation | null>(null);
  // @category TransientState
  let releaseQty = $state('');
  // @category TransientState
  let releaseReason: ReleaseReasonCode = $state('SupplierDelay');
  // @category TransientState
  let releaseNote = $state('');

  const REASON_OPTIONS: { value: ReleaseReasonCode; label: string }[] = [
    { value: 'SupplierNonPerformance', label: 'عدم تنفيذ التموين' },
    { value: 'SupplierDelay', label: 'تأخير في التموين' },
    { value: 'ServiceContinuity', label: 'استمرارية الخدمة' },
    { value: 'OtherAuthorized', label: 'أخرى (بترخيص)' },
  ];

  onMount(async () => {
    await loadAll();
  });

  async function loadAll() {
    await contractsOp.run(async () => {
      const [nextContracts, nextUnits, nextSuppliers, nextProducts, nextSettings] =
        await Promise.all([
          listContracts(),
          listUnits(''),
          listSuppliers(),
          listProducts(),
          getSettings(),
        ]);
      contracts = nextContracts;
      units = nextUnits;
      suppliers = nextSuppliers;
      products = nextProducts;
      settings = nextSettings;
      if (nextSettings) {
        newFiscalYear = nextSettings.current_year;
      }
      if (selectedContract) {
        await refreshDetail(selectedContract.id);
      }
    });
  }

  async function refreshDetail(contractId: string) {
    const [productRows, allocationRows] = await Promise.all([
      getContractProducts(contractId),
      listContractAllocations(contractId),
    ]);
    selectedProducts = productRows;
    selectedAllocations = allocationRows;
    const exceptions: Record<string, ContractAllocationException[]> = {};
    for (const allocation of allocationRows) {
      exceptions[allocation.id] = await listAllocationExceptions(allocation.id);
    }
    allocationExceptions = exceptions;
  }

  async function selectContract(contract: Contract) {
    selectedContract = contract;
    selectedProducts = [];
    selectedAllocations = [];
    allocationExceptions = {};
    const unitSuppliersResult = await listUnitSuppliers(contract.unit_id);
    unitSuppliers = unitSuppliersResult;
    await refreshDetail(contract.id);
  }

  async function handleUnitChange() {
    unitSuppliers = newUnitId ? await listUnitSuppliers(newUnitId) : [];
    newSupplierId = '';
  }

  function openCreate() {
    newUnitId = '';
    newSupplierId = '';
    newFiscalYear = settings?.current_year ?? new Date().getFullYear();
    newReference = '';
    newNotes = '';
    unitSuppliers = [];
    showCreateModal = true;
    contractsOp.error.set(null);
  }

  async function handleExport(kind: 'contracts' | 'allocations') {
    const filterName = 'Excel';
    const extension = 'xlsx';
    const defaultFilename = kind === 'contracts' ? 'العقود.xlsx' : 'استحقاقات_العقود.xlsx';
    const filePath = await saveFile({
      filters: [{ name: filterName, extensions: [extension] }],
      defaultPath: defaultFilename,
    });
    if (!filePath) return;
    await contractsOp.run(async () => {
      const result = kind === 'contracts'
        ? await exportContractsExcel(filePath, selectedContract?.unit_id ?? null, null, selectedContract?.fiscal_year ?? null)
        : await exportContractAllocationsExcel(filePath);
      if (result.success) {
        setSuccessWithTimeout(`تم التصدير بنجاح (${result.record_count})`);
      }
    });
  }

  function openDistribute() {
    distributeSelectedUnitIds = [];
    contractsOp.error.set(null);
    showDistributeModal = true;
  }

  function toggleDistributeUnit(unitId: string) {
    distributeSelectedUnitIds = distributeSelectedUnitIds.includes(unitId)
      ? distributeSelectedUnitIds.filter((id) => id !== unitId)
      : [...distributeSelectedUnitIds, unitId];
  }

  function toggleDistributeAll() {
    const allSelected = units.length > 0 && distributeSelectedUnitIds.length === units.length;
    distributeSelectedUnitIds = allSelected ? [] : units.map((u) => u.id);
  }

  async function handleDistribute() {
    if (units.length === 0 || distributeSelectedUnitIds.length === 0) {
      contractsOp.error.set('يرجى اختيار وحدة واحدة على الأقل لتوزيع كتالوج العقود');
      return;
    }
    const targetCodes: string[] = [];
    for (const id of distributeSelectedUnitIds) {
      const unit = units.find((u) => u.id === id);
      if (!unit?.code) {
        contractsOp.error.set(`تعذر تحديد رمز الوحدة «${id}»`);
        return;
      }
      targetCodes.push(unit.code);
    }
    const year = settings?.current_year ?? new Date().getFullYear();
    const filePath = await saveFile({
      defaultPath: `contract_catalog_${year}.sync`,
      filters: [{ name: 'حزمة المزامنة', extensions: ['sync'] }],
    });
    if (!filePath) return;
    const distributed = await contractsOp.run(async () => {
      const result = await exportContractCatalogToUnits(filePath, targetCodes);
      setSuccessWithTimeout(
        `تم توزيع كتالوج العقود على ${targetCodes.length} وحدة: ${result.record_count} سجل (حزمة .sync موقعة رقمياً)`
      );
      showDistributeModal = false;
      distributeSelectedUnitIds = [];
      return true;
    });
    if (distributed !== null) {
      await loadAll();
    }
  }

  async function saveContract() {
    if (!newUnitId || !newSupplierId || !newReference || !newFiscalYear) {
      contractsOp.error.set('يرجى ملء الحقول الإلزامية');
      return;
    }
    const created = await contractsOp.run(async () => {
      const request: CreateContractRequest = {
        unit_id: newUnitId,
        supplier_id: newSupplierId,
        fiscal_year: newFiscalYear,
        contract_reference: newReference.trim(),
        notes: newNotes.trim() || null,
      };
      const result = await createContract(request);
      setSuccessWithTimeout('تم إنشاء العقد بنجاح');
      showCreateModal = false;
      return result;
    });
    if (created !== null) {
      await loadAll();
      await selectContract(created);
    }
  }

  function openAddProduct() {
    newProductId = '';
    newProposedPrice = '';
    newAgreedPrice = '';
    newQuantity = '';
    showAddProductModal = true;
    contractsOp.error.set(null);
  }

  async function saveContractProduct() {
    const contract = selectedContract;
    if (!contract || !newProductId || !newProposedPrice || !newQuantity) {
      contractsOp.error.set('يرجى ملء الحقول الإلزامية');
      return;
    }
    const productAdded = await contractsOp.run(async () => {
      const agreed =
          newAgreedPrice === '' || newAgreedPrice === null
              ? null
              : parseFloat(newAgreedPrice);
      const request: AddContractProductRequest = {
        contract_id: contract.id,
        product_id: newProductId,
        proposed_price_ht: parseFloat(newProposedPrice),
        agreed_price_ht: agreed,
        contracted_quantity: parseFloat(newQuantity),
      };
      await addContractProduct(request);
      setSuccessWithTimeout('تمت إضافة المنتج إلى العقد');
      showAddProductModal = false;
    });
    if (productAdded !== null) await refreshDetail(contract.id);
  }

  function openSetAgreedPrice(product: ContractProduct) {
    agreedPriceTarget = product;
    contractsOp.error.set(null);
  }

  async function saveAgreedPrice() {
    const target = agreedPriceTarget;
    if (!target) return;
    const value = parseFloat(newAgreedPrice);
    if (Number.isNaN(value) || value < 0) {
      contractsOp.error.set('يرجى إدخال سعر اتفاق صحيح');
      return;
    }
    const priceOk = await contractsOp.run(async () => {
      await setContractProductAgreedPriceHt({
        contract_product_id: target.id,
        agreed_price_ht: value,
      });
      setSuccessWithTimeout(
        target.agreed_price_ht !== null ? 'تم تحديث سعر الاتفاق' : 'تم تثبيت سعر الاتفاق'
      );
      agreedPriceTarget = null;
    });
    if (priceOk !== null && selectedContract) await refreshDetail(selectedContract.id);
  }

  async function transition(action: 'accept' | 'activate' | 'end' | 'cancel') {
    const contract = selectedContract;
    if (!contract) return;
    const transitionOk = await contractsOp.run(async () => {
      if (action === 'accept') await acceptContract({ contract_id: contract.id });
      if (action === 'activate') await activateContract({ contract_id: contract.id });
      if (action === 'end') await endContract({ contract_id: contract.id });
      if (action === 'cancel') await cancelContract({ contract_id: contract.id });
      setSuccessWithTimeout('تم تحديث حالة العقد');
    });
    if (transitionOk !== null) {
      await loadAll();
      if (selectedContract) await selectContract(selectedContract);
    }
  }

  async function handleCancelConfirm() {
    const confirmed = await showAsk('تأكيد إلغاء العقد', {
      title: 'تأكيد الإلغاء',
      kind: 'warning',
      okLabel: 'نعم، ألغِ العقد',
      cancelLabel: 'رجوع',
    });
    if (confirmed) await transition('cancel');
  }

  function openRelease(allocation: ContractAllocation) {
    releaseTarget = allocation;
    releaseQty = '';
    releaseReason = 'SupplierDelay';
    releaseNote = '';
    contractsOp.error.set(null);
  }

  async function saveRelease() {
    const target = releaseTarget;
    if (!target) return;
    const qty = parseFloat(releaseQty);
    if (Number.isNaN(qty) || qty <= 0) {
      contractsOp.error.set('يرجى إدخال كمية إفراج صحيحة');
      return;
    }
    await contractsOp.run(async () => {
      const request: ReleaseContractAllocationRequest = {
        allocation_id: target.id,
        released_quantity: qty,
        reason_code: releaseReason,
        reason_note: releaseNote.trim() || null,
      };
      await releaseContractAllocation(request);
      setSuccessWithTimeout('تم تسجيل إفراج الالتزام');
      releaseTarget = null;
      if (selectedContract) await refreshDetail(selectedContract.id);
    });
  }

  async function handleRevoke(exception: ContractAllocationException) {
    await contractsOp.run(async () => {
      await revokeContractAllocationRelease({ exception_id: exception.id });
      setSuccessWithTimeout('تم إبطال الإفراج');
      if (selectedContract) await refreshDetail(selectedContract.id);
    });
  }

  function statusIntent(status: Contract['status']): 'success' | 'warning' | 'danger' | 'info' | 'neutral' {
    switch (status) {
      case 'Active': return 'success';
      case 'Proposed': return 'info';
      case 'Accepted': return 'warning';
      case 'Ended': return 'neutral';
      case 'Cancelled': return 'danger';
    }
  }

  function statusLabel(status: Contract['status']): string {
    switch (status) {
      case 'Proposed': return 'مقترح';
      case 'Accepted': return 'مقبول';
      case 'Active': return 'نشط';
      case 'Ended': return 'منتهي';
      case 'Cancelled': return 'ملغى';
    }
  }

  function entState(state: string): string {
    switch (state) {
      case 'ACTIVE': return 'نشط';
      case 'ENDED': return 'منتهي';
      case 'CANCELLED': return 'ملغى';
      default: return state;
    }
  }
</script>

<Layout nodeType="WILAYA" title="العقود" subtitle="عقود التموين والأذونات للوحدات">
  <div dir="rtl" class="mb-8">
    <AppPageHeader title="إدارة العقود" subtitle="إسناد الموردين والكميات والأسعار لكل وحدة وسنة مالية">
      <svelte:fragment slot="actions">
        <AppButton variant="secondary" on:click={() => handleExport('allocations')}>تصدير الاستحقاقات</AppButton>
        <AppButton variant="secondary" on:click={() => handleExport('contracts')}>تصدير العقود</AppButton>
        <AppButton
          variant="secondary"
          on:click={openDistribute}
          ariaLabel="توزيع كتالوج العقود على الوحدات المحددة كحزم مزامنة موقعة"
        >
          توزيع الكتالوج على الوحدات
        </AppButton>
        <AppButton on:click={openCreate} ariaLabel="إنشاء عقد جديد">إنشاء عقد</AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if success}
      <div class="mt-4 p-3 rounded-lg bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-300 text-sm" role="status">
        {success}
      </div>
    {/if}

    <div class="mt-6 grid grid-cols-1 lg:grid-cols-2 gap-6">
      <AppCard>
        <h2 class="text-sm font-bold text-gray-700 dark:text-gray-200 mb-3">قائمة العقود</h2>
        <AppTable loading={$loading} empty={contracts.length === 0} error={$error} emptyMessage="لا توجد عقود بعد" caption="قائمة العقود">
          <svelte:fragment slot="head">
            <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">المرجع</th>
            <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الوحدة</th>
            <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">المورد</th>
            <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">السنة</th>
            <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الحالة</th>
          </svelte:fragment>
          {#each contracts as contract (contract.id)}
            <tr
              class="border-t border-gray-100 dark:border-gray-700 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700/50 {selectedContract?.id === contract.id ? 'bg-civil-blue/10 dark:bg-civil-blue/20' : ''}"
              onclick={() => selectContract(contract)}
            >
              <td class="px-4 py-3 text-sm text-gray-800 dark:text-gray-100">{contract.contract_reference}</td>
              <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">{orgUnitLabel(contract.unit_id)}</td>
              <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">
                {suppliers.find((s) => s.id === contract.supplier_id)?.name || contract.supplier_id}
              </td>
              <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">{contract.fiscal_year}</td>
              <td class="px-4 py-3"><AppBadge intent={statusIntent(contract.status)}>{statusLabel(contract.status)}</AppBadge></td>
            </tr>
          {/each}
          <svelte:fragment slot="empty">
            <AppEmptyState title="لا توجد عقود" description="أنشئ عقدًا لإسناد مورد وكمية لوحدة.">
              <svelte:fragment slot="action">
                <AppButton on:click={openCreate}>إنشاء أول عقد</AppButton>
              </svelte:fragment>
            </AppEmptyState>
          </svelte:fragment>
        </AppTable>
      </AppCard>

      <AppCard>
        <h2 class="text-sm font-bold text-gray-700 dark:text-gray-200 mb-3">{selectedContract ? `تفاصيل: ${selectedContract.contract_reference}` : 'تفاصيل العقد'}</h2>
        {#if selectedContract}
          {#if $error}
            <div class="mb-4"><AppAlert intent="danger" dismissible on:dismiss={() => contractsOp.error.set(null)}>{$error}</AppAlert></div>
          {/if}
          <div class="flex flex-wrap items-center gap-2 mb-4">
            <AppBadge intent={statusIntent(selectedContract.status)}>{statusLabel(selectedContract.status)}</AppBadge>
            {#if selectedContract.status === 'Proposed'}
              <AppButton size="sm" variant="secondary" on:click={() => transition('accept')}>قبول</AppButton>
            {/if}
            {#if selectedContract.status === 'Accepted'}
              <AppButton size="sm" variant="primary" on:click={() => transition('activate')}>تفعيل</AppButton>
            {:else if selectedContract.status === 'Active'}
              <AppButton size="sm" variant="primary" on:click={() => transition('end')}>إنهاء</AppButton>
            {/if}
            {#if selectedContract.status === 'Proposed' || selectedContract.status === 'Accepted' || selectedContract.status === 'Active'}
              <AppButton size="sm" variant="danger" on:click={handleCancelConfirm}>إلغاء</AppButton>
            {/if}
          </div>

          <div class="flex items-center justify-between gap-3 mb-2">
            <h3 class="text-sm font-bold text-gray-700 dark:text-gray-200">منتجات العقد</h3>
            <AppProductSearch bind:search={contractProductsSearch} />
          </div>
          <AppTable
            empty={filteredSelectedProducts.length === 0}
            emptyMessage={contractProductsSearch.trim() ? 'لا توجد نتائج مطابقة' : 'لم تُضف منتجات بعد'}
          >
            <svelte:fragment slot="head">
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">المنتج</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الكمية المتفق عليها</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">السعر المقترح</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">التسعير المتفق عليه</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">نسبة الضريبة</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">السعر ش.ض (TTC)</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">إجراءات</th>
            </svelte:fragment>
            {#each filteredSelectedProducts as product (product.id)}
              <tr class="border-t border-gray-100 dark:border-gray-700">
                <td class="px-3 py-2 text-sm text-gray-800 dark:text-gray-100">{product.product_name}</td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">
                  {contractedQtyForProduct(product.id) !== null
                    ? contractedQtyForProduct(product.id)?.toFixed(2)
                    : '—'}
                </td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">{product.proposed_price_ht.toFixed(2)} دج</td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">
                  {#if selectedContract.status === 'Proposed' && product.agreed_price_ht !== null}
                    <span class="me-1">{product.agreed_price_ht.toFixed(2)} دج</span>
                    <AppButton size="sm" variant="secondary" on:click={() => { newAgreedPrice = (product.agreed_price_ht ?? 0).toString(); openSetAgreedPrice(product); }}>مراجعة السعر</AppButton>
                  {:else if product.agreed_price_ht !== null}
                    {product.agreed_price_ht.toFixed(2)} دج
                  {:else if selectedContract.status === 'Proposed'}
                    <AppButton size="sm" variant="secondary" on:click={() => { newAgreedPrice = ''; openSetAgreedPrice(product); }}>تثبيت السعر</AppButton>
                  {:else}
                    —
                  {/if}
                </td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">
                  {#if product.tva_rate !== null}
                    {product.tva_rate.toFixed(2)}%
                  {:else}
                    —
                  {/if}
                </td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">
                  {#if product.price_ttc !== null}
                    {product.price_ttc.toFixed(2)} دج
                  {:else}
                    —
                  {/if}
                </td>
                <td class="px-3 py-2 text-sm">
                  {#if product.agreed_price_ht === null && selectedContract.status === 'Proposed'}
                    <AppButton size="sm" variant="ghost" on:click={() => { newAgreedPrice = product.proposed_price_ht.toString(); openSetAgreedPrice(product); }}>اعتماد المقترح</AppButton>
                  {/if}
                </td>
              </tr>
            {/each}
          </AppTable>
          {#if selectedContract.status === 'Proposed' || selectedContract.status === 'Accepted'}
            <div class="mt-3">
              <AppButton size="sm" variant="secondary" on:click={openAddProduct}>إضافة منتج</AppButton>
            </div>
          {/if}

          <div class="flex items-center justify-between gap-3 mt-6 mb-2">
            <h3 class="text-sm font-bold text-gray-700 dark:text-gray-200">أذونات الوحدات (الالتزامات)</h3>
            <AppProductSearch bind:search={allocationsSearch} />
          </div>
          <AppTable
            empty={filteredSelectedAllocations.length === 0}
            emptyMessage={allocationsSearch.trim() ? 'لا توجد نتائج مطابقة' : 'لا توجد أذونات بعد'}
          >
            <svelte:fragment slot="head">
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">المنتج</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الكمية المتفق عليها</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">المتبقي الفعّال</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الحالة</th>
              <th class="px-3 py-2 text-right text-xs font-bold text-gray-500 dark:text-gray-400">إجراءات</th>
            </svelte:fragment>
            {#each filteredSelectedAllocations as allocation (allocation.id)}
              <tr class="border-t border-gray-100 dark:border-gray-700">
                <td class="px-3 py-2 text-sm text-gray-800 dark:text-gray-100">
                  {allocationProductName(allocation)}
                </td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">{allocation.contracted_quantity}</td>
                <td class="px-3 py-2 text-sm font-semibold text-gray-800 dark:text-gray-100">
                  {allocation.effective_remaining.toFixed(2)}
                </td>
                <td class="px-3 py-2 text-sm text-gray-500 dark:text-gray-400">{entState(allocation.entitlement_state)}</td>
                <td class="px-3 py-2">
                  {#if selectedContract.status === 'Active' || selectedContract.status === 'Ended'}
                    <AppButton size="sm" variant="secondary" on:click={() => openRelease(allocation)}>إفراج</AppButton>
                  {/if}
                </td>
              </tr>
              {#if (allocationExceptions[allocation.id] ?? []).length > 0}
                <tr class="border-t border-gray-100 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
                  <td colspan="5" class="px-3 py-2">
                    <ul class="space-y-1">
                      {#each (allocationExceptions[allocation.id] ?? []) as exception (exception.id)}
                        <li class="flex items-center justify-between text-xs text-gray-600 dark:text-gray-300">
                          <span>
                            إفراج {exception.released_quantity} — {exception.reason_code}
                            {exception.reason_note ? ` — ${exception.reason_note}` : ''} ({new Date(exception.created_at).toLocaleString('fr-FR')})
                          </span>
                          <AppButton size="sm" variant="ghost" on:click={() => handleRevoke(exception)}>إبطال</AppButton>
                        </li>
                      {/each}
                    </ul>
                  </td>
                </tr>
              {/if}
            {/each}
          </AppTable>
        {:else}
          <AppEmptyState title="اختر عقدًا" description="حدد عقدًا من القائمة لعرض تفاصيله وأذوناته." />
        {/if}
      </AppCard>
    </div>
  </div>

  <AppDialog open={showCreateModal} title="إنشاء عقد جديد" description="إسناد مورد لوحدة لسنة مالية (عقد واحد نشط لكل وحدة ومنتج وسنة)" on:close={() => (showCreateModal = false)}>
    <div dir="rtl" class="space-y-4">
      <AppSelect id="contract-unit" label="الوحدة *" bind:value={newUnitId} required on:change={handleUnitChange}>
        <option value="">— اختر الوحدة —</option>
        {#each units as unit (unit.id)}
          <option value={unit.id}>{unit.name} ({unit.code})</option>
        {/each}
      </AppSelect>
      <AppSelect id="contract-supplier" label="المورد *" bind:value={newSupplierId} required>
        <option value="">{unitSuppliers.length > 0 ? '— اختر المورد —' : '— لا يوجد مورد مرتبط بهذه الوحدة —'}</option>
        {#each unitSuppliers as supplier (supplier.id)}
          <option value={supplier.id}>{supplier.name}</option>
        {/each}
      </AppSelect>
      <AppInput id="contract-ref" label="المرجع *" bind:value={newReference} required placeholder="مثال: C-2026-001" />
      <AppInput id="contract-year" label="السنة المالية *" type="number" bind:value={newFiscalYear} required min={2020} max={2100} />
      <AppInput id="contract-notes" label="ملاحظات" bind:value={newNotes} placeholder="ملاحظات اختيارية" />
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={() => (showCreateModal = false)}>إلغاء</AppButton>
      <AppButton on:click={saveContract} loading={$loading}>إنشاء</AppButton>
    </svelte:fragment>
  </AppDialog>

  <AppDialog open={showAddProductModal} title="إضافة منتج للعقد" on:close={() => (showAddProductModal = false)}>
    <div dir="rtl" class="space-y-4">
      <AppSelect id="add-product-id" label="المنتج *" bind:value={newProductId} required>
        <option value="">— اختر المنتج —</option>
        {#each products as product (product.id)}
          <option value={product.id}>{product.name}</option>
        {/each}
      </AppSelect>
      <AppInput id="add-proposed-price" label="السعر المقترح (دج) — من الكتالوج" type="number" value={newProposedPrice} readonly required min={0} placeholder="0.00" />
      <AppInput id="add-agreed-price" label="سعر الاتفاق (دج، اختياري)" type="number" bind:value={newAgreedPrice} min={0} placeholder="0.00" />
      {#if newProductId}
        <div class="rounded-md bg-gray-50 dark:bg-gray-800 px-3 py-2 text-xs text-gray-600 dark:text-gray-400">
          <div class="flex items-center justify-between gap-2">
            <span>نسبة الضريبة</span>
            <span class="font-semibold">{addProductTvaRate !== null ? addProductTvaRate.toFixed(2) + '%' : '—'}</span>
          </div>
          {#if newAgreedPrice !== '' && newAgreedPrice !== '.' && addProductTtcPreview !== ''}
            <div class="mt-1 flex items-center justify-between gap-2 border-t border-gray-200 dark:border-gray-700 pt-1">
              <span>السعر المقدر ش.ض (TTC، غير ملزم)</span>
              <span class="font-semibold">{addProductTtcPreview} دج</span>
            </div>
          {/if}
        </div>
      {/if}
      <AppInput id="add-quantity" label={orderQuantityLabel()} type="number" bind:value={newQuantity} required min={0} placeholder="0" />
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={() => (showAddProductModal = false)}>إلغاء</AppButton>
      <AppButton on:click={saveContractProduct} loading={$loading}>إضافة</AppButton>
    </svelte:fragment>
  </AppDialog>

  <AppDialog
    open={agreedPriceTarget !== null}
    title={agreedPriceTarget?.agreed_price_ht !== null ? 'مراجعة سعر الاتفاق' : 'تثبيت سعر الاتفاق'}
    on:close={() => (agreedPriceTarget = null)}
  >
    <div dir="rtl" class="space-y-4">
      <AppInput id="agreed-price" label="سعر الاتفاق (دج) *" type="number" bind:value={newAgreedPrice} required min={0} placeholder="0.00" />
      {#if agreedPriceTarget}
        <div class="rounded-md bg-gray-50 dark:bg-gray-800 px-3 py-2 text-xs text-gray-600 dark:text-gray-400">
          <div class="flex items-center justify-between gap-2">
            <span>نسبة الضريبة</span>
            <span class="font-semibold">{agreedPriceTvaRate !== null ? agreedPriceTvaRate.toFixed(2) + '%' : '—'}</span>
          </div>
          {#if newAgreedPrice !== '' && newAgreedPrice !== '.' && agreedPriceTtcRecap !== ''}
            <div class="mt-1 flex items-center justify-between gap-2 border-t border-gray-200 dark:border-gray-700 pt-1">
              <span>السعر ش.ض (TTC، غير ملزم)</span>
              <span class="font-semibold">{agreedPriceTtcRecap} دج</span>
            </div>
          {/if}
        </div>
      {/if}
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={() => (agreedPriceTarget = null)}>إلغاء</AppButton>
      <AppButton on:click={saveAgreedPrice} loading={$loading}>
        {agreedPriceTarget?.agreed_price_ht !== null ? 'تحديث' : 'تثبيت'}
      </AppButton>
    </svelte:fragment>
  </AppDialog>

  <AppDialog open={releaseTarget !== null} title="إفراج التزام" description="تحرير كمية من الالتزام بسبب استثناء (ولاية فقط — مسجّل تدقيقيًا)" on:close={() => (releaseTarget = null)}>
    <div dir="rtl" class="space-y-4">
      <AppInput id="release-qty" label="الكمية المفرج عنها *" type="number" bind:value={releaseQty} required min={0} placeholder="0" />
      <AppSelect id="release-reason" label="سبب الإفراج *" bind:value={releaseReason} required>
        {#each REASON_OPTIONS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </AppSelect>
      <AppInput id="release-note" label="ملاحظة" bind:value={releaseNote} placeholder="تفاصيل الاستثناء (اختياري)" />
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={() => (releaseTarget = null)}>إلغاء</AppButton>
      <AppButton on:click={saveRelease} loading={$loading}>تسجيل الإفراج</AppButton>
    </svelte:fragment>
  </AppDialog>

  <AppDialog
    open={showDistributeModal}
    title="توزيع الكتالوج على الوحدات"
    description="اختر الوحدات المستهدفة — تصدر لكل وحدة حزمة .sync موقعة ومشفرة خاصة بها (نمط التوزيع على الوحدة)"
    on:close={() => (showDistributeModal = false)}
  >
    <div dir="rtl" class="space-y-4">
      <label class="flex items-center gap-3 p-2 rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700/50">
        <input
          type="checkbox"
          checked={units.length > 0 && distributeSelectedUnitIds.length === units.length}
          onchange={toggleDistributeAll}
          disabled={units.length === 0}
          class="w-4 h-4 rounded"
        />
        <span class="text-sm font-medium text-gray-800 dark:text-gray-200">تحديد جميع الوحدات</span>
      </label>
      {#if units.length === 0}
        <AppAlert intent="info">لا توجد وحدات مسجلة بعد. أنشئ الوحدات أولاً من صفحة "الوحدات".</AppAlert>
      {:else}
        <div class="max-h-[40vh] overflow-y-auto space-y-1 border border-gray-200 dark:border-gray-700 rounded-lg p-3">
          {#each units as unit (unit.id)}
            <label class="flex items-center gap-3 p-2 rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700/50">
              <input
                type="checkbox"
                checked={distributeSelectedUnitIds.includes(unit.id)}
                onchange={() => toggleDistributeUnit(unit.id)}
                class="w-4 h-4 rounded"
              />
              <span class="text-sm font-medium text-gray-800 dark:text-gray-200">{unit.code} - {unit.name}</span>
            </label>
          {/each}
        </div>
      {/if}
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={() => (showDistributeModal = false)}>إلغاء</AppButton>
      <AppButton on:click={handleDistribute} loading={$loading} disabled={distributeSelectedUnitIds.length === 0}>
        توزيع ({distributeSelectedUnitIds.length})
      </AppButton>
    </svelte:fragment>
  </AppDialog>
</Layout>