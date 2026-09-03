<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { saveFile, showAsk } from '../lib/tauri';
  import {
    listSuppliers,
    createSupplier,
    updateSupplier,
    setSupplierActive,
    exportSuppliersExcel,
    listUnits,
    listUnitSuppliers,
    associateSupplierWithUnit,
    disassociateSupplierFromUnit,
    getSettings,
  } from '../lib/contracts';
  import type { Supplier, Unit, CreateSupplierRequest, UpdateSupplierRequest } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  const suppliersOp = createOperation({ scope });
  const loading = suppliersOp.loading;
  const error = suppliersOp.error;

  // @category ProjectionState
  let suppliers = $state<Supplier[]>([]);
  // @category ProjectionState
  let units = $state<Unit[]>([]);
  // @category ProjectionState
  let supplierUnits = $state<Record<string, string[]>>({});
  // @category UiState
  let showModal = $state(false);
  // @category UiState
  let showAssociationModal = $state(false);
  // @category UiState
  let associationSupplier = $state<Supplier | null>(null);
  // @category UiState
  let editingSupplier = $state<Supplier | null>(null);
  // @category TransientState
  let success = $state('');
  // @category TransientState
  let selectedUnitIds = $state<string[]>([]);

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m));
  onDestroy(() => scope.dispose());

  // Form fields
  // @category TransientState
  let supplierName = $state('');
  // @category TransientState
  let contactInfo = $state('');

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    await suppliersOp.run(async () => {
      const [nextSuppliers, settings] = await Promise.all([listSuppliers(), getSettings()]);
      suppliers = nextSuppliers;
      units = settings?.wilaya_code ? await listUnits(settings.wilaya_code) : [];

      const map: Record<string, string[]> = {};
      for (const unit of units) {
        const unitSuppliers = await listUnitSuppliers(unit.id);
        for (const s of unitSuppliers) {
          (map[s.id] ??= []).push(unit.id);
        }
      }
      supplierUnits = map;
    });
  }

  function openCreateModal() {
    editingSupplier = null;
    supplierName = '';
    contactInfo = '';
    showModal = true;
    suppliersOp.error.set(null);
  }

  function openEditModal(supplier: Supplier) {
    editingSupplier = supplier;
    supplierName = supplier.name;
    contactInfo = supplier.contact_info || '';
    showModal = true;
    suppliersOp.error.set(null);
  }

  function closeModal() {
    showModal = false;
    editingSupplier = null;
    suppliersOp.error.set(null);
  }

  async function saveSupplier() {
    if (!supplierName) {
      suppliersOp.error.set('يرجى ملء اسم المورد');
      return;
    }

    await suppliersOp.run(async () => {
      if (editingSupplier) {
        const request: UpdateSupplierRequest = {
          id: editingSupplier.id,
          name: supplierName.trim(),
          contact_info: contactInfo.trim() || null,
        };
        await updateSupplier(request);
        setSuccessWithTimeout('تم تحديث المورد بنجاح');
      } else {
        const request: CreateSupplierRequest = {
          name: supplierName.trim(),
          contact_info: contactInfo.trim() || null,
        };
        await createSupplier(request);
        setSuccessWithTimeout('تم إنشاء المورد بنجاح');
      }

      closeModal();
      await loadData();
    });
  }

  async function toggleActive(supplier: Supplier) {
    await suppliersOp.run(async () => {
      await setSupplierActive({ supplier_id: supplier.id, active: !supplier.active });
      setSuccessWithTimeout(supplier.active ? 'تم إيقاف المورد' : 'تم تفعيل المورد');
      await loadData();
    });
  }

  async function handleDelete(supplier: Supplier) {
    const confirmed = await showAsk(
      `هل أنت متأكد من إيقاف المورد "${supplier.name}"؟ يبقى السجل محفوظًا للتدقيق التاريخي.`,
      { title: 'تأكيد الإيقاف', kind: 'warning', okLabel: 'نعم', cancelLabel: 'لا' },
    );
    if (!confirmed) return;
    await toggleActive(supplier);
  }

  function openAssociation(supplier: Supplier) {
    associationSupplier = supplier;
    selectedUnitIds = [...(supplierUnits[supplier.id] ?? [])];
    suppliersOp.error.set(null);
    showAssociationModal = true;
  }

  function closeAssociation() {
    showAssociationModal = false;
    associationSupplier = null;
    selectedUnitIds = [];
    suppliersOp.error.set(null);
  }

  function toggleUnit(unitId: string) {
    selectedUnitIds = selectedUnitIds.includes(unitId)
      ? selectedUnitIds.filter((id) => id !== unitId)
      : [...selectedUnitIds, unitId];
  }

  function getUnitCode(unitId: string): string {
    return units.find((u) => u.id === unitId)?.code ?? unitId;
  }

  async function saveAssociations() {
    if (!associationSupplier) return;
    const current = supplierUnits[associationSupplier.id] ?? [];
    const next = new Set(selectedUnitIds);
    const toAdd = selectedUnitIds.filter((id) => !current.includes(id));
    const toRemove = current.filter((id) => !next.has(id));

    await suppliersOp.run(async () => {
      for (const unitId of toAdd) {
        await associateSupplierWithUnit({ unit_id: unitId, supplier_id: associationSupplier!.id });
      }
      for (const unitId of toRemove) {
        await disassociateSupplierFromUnit({ unit_id: unitId, supplier_id: associationSupplier!.id });
      }
      setSuccessWithTimeout('تم تحديث ارتباطات المورد بالوحدات بنجاح');
      closeAssociation();
      await loadData();
    });
  }

  async function handleExport() {
    const filePath = await saveFile({
      filters: [{ name: 'Excel', extensions: ['xlsx'] }],
      defaultPath: `الموردون.xlsx`,
    });
    if (!filePath) return;
    await suppliersOp.run(async () => {
      const result = await exportSuppliersExcel(filePath);
      if (result.success) {
        setSuccessWithTimeout(`تم تصدير الموردين بنجاح (${result.record_count})`);
      }
    });
  }
</script>

<Layout nodeType="WILAYA" title="الموردون" subtitle="إدارة موردي التموين على مستوى الولاية">
  <div dir="rtl" class="mb-8">
    <AppPageHeader title="إدارة الموردين" subtitle="قاعدة إسناد عقود التموين للوحدات">
      <svelte:fragment slot="actions">
        <AppButton variant="secondary" on:click={handleExport}>تصدير Excel</AppButton>
        <AppButton on:click={openCreateModal} ariaLabel="إضافة مورد جديد">
          إضافة مورد
        </AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if success}
      <div class="mt-4 p-3 rounded-lg bg-green-50 dark:bg-green-900/30 text-green-700 dark:text-green-300 text-sm" role="status">
        {success}
      </div>
    {/if}

    <div class="mt-6">
      <AppTable loading={$loading} empty={suppliers.length === 0} error={$error} emptyMessage="لا يوجد موردون بعد" caption="قائمة الموردين">
        <svelte:fragment slot="head">
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الاسم</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">معلومات الاتصال</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الوحدات المرتبطة</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الحالة</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">تاريخ الإنشاء</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">إجراءات</th>
        </svelte:fragment>
        {#each suppliers as supplier (supplier.id)}
          <tr class="border-t border-gray-100 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <td class="px-4 py-3 text-sm text-gray-800 dark:text-gray-100">{supplier.name}</td>
            <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">{supplier.contact_info || '—'}</td>
            <td class="px-4 py-3">
              {#if (supplierUnits[supplier.id] ?? []).length > 0}
                <div class="flex flex-wrap gap-1 max-w-[180px]">
                  {#each supplierUnits[supplier.id] as unitId (unitId)}
                    <AppBadge intent="neutral" size="sm">{getUnitCode(unitId)}</AppBadge>
                  {/each}
                </div>
              {:else}
                <span class="text-sm text-gray-400">غير مرتبطة</span>
              {/if}
            </td>
            <td class="px-4 py-3">
              {#if supplier.active}
                <AppBadge intent="success">نشط</AppBadge>
              {:else}
                <AppBadge intent="danger">موقوف</AppBadge>
              {/if}
            </td>
            <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">
              {new Date(supplier.created_at).toLocaleDateString('fr-FR')}
            </td>
            <td class="px-4 py-3">
              <div class="flex items-center gap-2">
                <AppButton size="sm" variant="secondary" on:click={() => openEditModal(supplier)}>تعديل</AppButton>
                <AppButton size="sm" variant="ghost" on:click={() => openAssociation(supplier)} ariaLabel={`إدارة الوحدات المرتبطة بالمورد ${supplier.name}`}>
                  ربط الوحدات
                </AppButton>
                <AppButton
                  size="sm"
                  variant={supplier.active ? 'danger' : 'primary'}
                  on:click={() => handleDelete(supplier)}
                >
                  {supplier.active ? 'إيقاف' : 'تفعيل'}
                </AppButton>
              </div>
            </td>
          </tr>
        {/each}
        <svelte:fragment slot="empty">
          <AppEmptyState title="لا يوجد موردون" description="ابدأ بإضافة مورد لإسناد عقود التموين له.">
            <svelte:fragment slot="action">
              <AppButton on:click={openCreateModal}>إضافة أول مورد</AppButton>
            </svelte:fragment>
          </AppEmptyState>
        </svelte:fragment>
      </AppTable>
    </div>
  </div>

  <AppDialog
    open={showModal}
    title={editingSupplier ? 'تعديل مورد' : 'إضافة مورد جديد'}
    description="بيانات المورّد — تُستخدم لإسناد عقود التموين لكل وحدة."
    on:close={closeModal}
  >
    <div dir="rtl">
      <div class="space-y-4">
        <AppInput
          id="supplier-name"
          label="اسم المورّد *"
          bind:value={supplierName}
          required
          placeholder="مثال: تعاونية التموين الجهوية"
        />
        <AppInput
          id="supplier-contact"
          label="معلومات الاتصال"
          bind:value={contactInfo}
          placeholder="الهاتف / البريد / العنوان"
        />
      </div>
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
      <AppButton on:click={saveSupplier} loading={$loading}>
        {editingSupplier ? 'حفظ التعديلات' : 'إضافة'}
      </AppButton>
    </svelte:fragment>
  </AppDialog>

  <AppDialog
    open={showAssociationModal}
    title="ربط المورد بالوحدات"
    description="التزمات التموين للمورد تسري فقط على الوحدات المرتبطة به."
    on:close={closeAssociation}
  >
    <div dir="rtl">
      {#if associationSupplier}
        <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
          المورد: <span class="font-semibold text-gray-800 dark:text-gray-200">{associationSupplier.name}</span>
        </p>
        <p class="text-xs text-gray-400 mb-4">
          الوحدات المرتبطة فقط هي التي يستطيع النظام تحويل الطلبيات لها عند إنشاء أمر شراء. الارتباطات غير رجوعية من جانب الوحدة.
        </p>
      {/if}
      {#if units.length === 0}
        <AppAlert intent="info">لا توجد وحدات مسجلة بعد. أنشئ الوحدات أولاً من صفحة "الوحدات".</AppAlert>
      {:else}
        <div class="max-h-[40vh] overflow-y-auto space-y-1 border border-gray-200 dark:border-gray-700 rounded-lg p-3">
          {#each units as unit (unit.id)}
            <label class="flex items-center gap-3 p-2 rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700/50">
              <input
                type="checkbox"
                checked={selectedUnitIds.includes(unit.id)}
                onchange={() => toggleUnit(unit.id)}
                class="w-4 h-4 rounded"
              />
              <span class="text-sm font-medium text-gray-800 dark:text-gray-200">{unit.code} - {unit.name}</span>
            </label>
          {/each}
        </div>
      {/if}
    </div>
    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={closeAssociation}>إلغاء</AppButton>
      <AppButton on:click={saveAssociations} loading={$loading} disabled={units.length === 0}>حفظ الارتباطات</AppButton>
    </svelte:fragment>
  </AppDialog>
</Layout>