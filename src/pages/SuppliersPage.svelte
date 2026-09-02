<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { showAsk } from '../lib/tauri';
  import {
    listSuppliers,
    createSupplier,
    updateSupplier,
    setSupplierActive,
  } from '../lib/contracts';
  import type { Supplier, CreateSupplierRequest, UpdateSupplierRequest } from '../lib/types';
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
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  const suppliersOp = createOperation({ scope });
  const loading = suppliersOp.loading;
  const error = suppliersOp.error;

  // @category ProjectionState
  let suppliers = $state<Supplier[]>([]);
  // @category UiState
  let showModal = $state(false);
  // @category UiState
  let editingSupplier = $state<Supplier | null>(null);
  // @category TransientState
  let success = $state('');

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
      const nextSuppliers = await listSuppliers();
      suppliers = nextSuppliers;
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
</script>

<Layout nodeType="WILAYA" title="الموردون" subtitle="إدارة موردي التموين على مستوى الولاية">
  <div dir="rtl" class="mb-8">
    <AppPageHeader title="إدارة الموردين" subtitle="قاعدة إسناد عقود التموين للوحدات">
      <svelte:fragment slot="actions">
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
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">الحالة</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">تاريخ الإنشاء</th>
          <th class="px-4 py-3 text-right text-xs font-bold text-gray-500 dark:text-gray-400">إجراءات</th>
        </svelte:fragment>
        {#each suppliers as supplier (supplier.id)}
          <tr class="border-t border-gray-100 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <td class="px-4 py-3 text-sm text-gray-800 dark:text-gray-100">{supplier.name}</td>
            <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">{supplier.contact_info || '—'}</td>
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
</Layout>