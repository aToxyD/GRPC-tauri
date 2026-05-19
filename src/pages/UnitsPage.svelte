<script lang="ts">
  import { onMount } from "svelte";
  import {
    listUnits,
    createUnit,
    getSettings,
    exportUnitNodePackage,
    updateUnit,
    deleteUnit,
  } from "../lib/tauri";
  import { save } from "@tauri-apps/plugin-dialog";
  import type { Unit, Settings, CreateUnitRequest } from "../lib/types";
  import Layout from "../components/Layout.svelte";

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  let units: Unit[] = [];
  let settings: Settings | null = null;
  let loading = true;
  let showModal = false;
  let showEditModal = false;
  let showDeleteModal = false;
  let error = "";
  let success = "";
  let editingUnit: Unit | null = null;
  let deletingUnit: Unit | null = null;

  // Form fields
  let unitCode = "";
  let unitName = "";
  let username = "";
  let password = "";
  let confirmPassword = "";

  onMount(async () => {
    loadData();
  });

  async function loadData() {
    try {
      loading = true;
      settings = await getSettings();
      if (settings && settings.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
      }
    } catch (e) {
      error = "خطأ في التحميل: " + String(e);
    } finally {
      loading = false;
    }
  }

  function openCreateModal() {
    unitCode = "";
    unitName = "";
    username = "";
    password = "";
    confirmPassword = "";
    showModal = true;
    error = "";
  }

  function openEditModal(unit: Unit) {
    editingUnit = unit;
    unitCode = unit.code;
    unitName = unit.name;
    username = "";
    password = "";
    confirmPassword = "";
    showEditModal = true;
    error = "";
  }

  function openDeleteModal(unit: Unit) {
    deletingUnit = unit;
    showDeleteModal = true;
    error = "";
  }

  function closeModal() {
    showModal = false;
    showEditModal = false;
    showDeleteModal = false;
    editingUnit = null;
    deletingUnit = null;
    error = "";
  }

  async function saveUnit() {
    if (!unitCode || !unitName || !username || !password) {
      error = "الرجاء إدخال رمز واسم الوحدة";
      return;
    }

    if (password !== confirmPassword) {
      error = "كلمات المرور غير متطابقة";
      return;
    }

    if (!settings?.wilaya_code) {
      error = "تكوين الولاية غير موجود";
      return;
    }

    try {
      const request: CreateUnitRequest = {
        code: unitCode,
        name: unitName,
        username,
        password,
      };
      await createUnit(request, settings.wilaya_code);
      success = `تم إنشاء الوحدة ${unitCode} بنجاح.`;
      closeModal();
      loadData();
      setTimeout(() => (success = ""), 3000);
    } catch (e) {
      error = "خطأ: " + String(e);
    }
  }

  async function updateUnitData() {
    if (!unitCode || !unitName) {
      error = "الرجاء إدخال رمز واسم الوحدة";
      return;
    }

    if (!editingUnit) {
      error = "لم يتم تحديد الوحدة للتعديل";
      return;
    }

    try {
      const request: CreateUnitRequest = {
        code: unitCode,
        name: unitName,
        username: username || "",
        password: password || "",
      };
      await updateUnit(editingUnit.id, request);
      success = `تم تحديث الوحدة ${unitCode} بنجاح.`;
      closeModal();
      loadData();
      setTimeout(() => (success = ""), 3000);
    } catch (e) {
      error = "خطأ: " + String(e);
    }
  }

  async function deleteUnitData() {
    if (!deletingUnit) {
      error = "لم يتم تحديد الوحدة للحذف";
      return;
    }

    try {
      await deleteUnit(deletingUnit.id);
      success = `تم حذف الوحدة ${deletingUnit.code} بنجاح.`;
      closeModal();
      loadData();
      setTimeout(() => (success = ""), 3000);
    } catch (e) {
      error = "خطأ: " + String(e);
    }
  }

  async function handleExportPackage(unit: Unit) {
    error = "";
    try {
      const filePath = await save({
        filters: [
          {
            name: "حزمة التكوين",
            extensions: ["unit"],
          },
        ],
        defaultPath: `${unit.code}_package.unit`,
      });

      if (filePath) {
        const result = await exportUnitNodePackage(unit.id, filePath);
        if (result.success) {
          success = `تم تصدير حزمة الوحدة ${unit.code} بنجاح.`;
          setTimeout(() => (success = ""), 3000);
        } else {
          error = result.message?.trim()
            ? result.message
            : "فشل التصدير دون رسالة من الخادم.";
        }
      }
    } catch (e) {
      error = "خطأ في التصدير: " + String(e);
    }
  }
</script>

<Layout nodeType="WILAYA" title="إدارة الوحدات" subtitle="إنشاء وتكوين الوحدات">
  <div dir="rtl">
    <AppPageHeader title="إدارة الوحدات" subtitle="إنشاء وتكوين الوحدات">
      <svelte:fragment slot="actions">
        <AppButton variant="primary" on:click={openCreateModal}>
          <svg class="w-4 h-4 mr-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
          </svg>
          وحدة جديدة
        </AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => error = ''}>{error}</AppAlert>
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => success = ''}>{success}</AppAlert>
      </div>
    {/if}

    <AppCard padding="none">
      <AppTable {loading} empty={units.length === 0}>
        <svelte:fragment slot="empty">
          <AppEmptyState
            title="لا يوجد وحدات مسجلة"
            description="أنشئ وحدتك الأولى للبدء في المزامنة."
            icon="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
          >
            <svelte:fragment slot="action">
              <AppButton variant="primary" on:click={openCreateModal}>إنشاء أول وحدة</AppButton>
            </svelte:fragment>
          </AppEmptyState>
        </svelte:fragment>

        <svelte:fragment slot="head">
          <th class="table-header">الرمز</th>
          <th class="table-header">الاسم</th>
          <th class="table-header">تاريخ الإنشاء</th>
          <th class="table-header text-left">الإجراءات</th>
        </svelte:fragment>

        {#each units as unit}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
            <td class="table-cell font-medium">{unit.code}</td>
            <td class="table-cell">{unit.name}</td>
            <td class="table-cell">{new Date(unit.created_at).toLocaleDateString("ar-EG")}</td>
            <td class="table-cell text-left space-x-2 space-x-reverse">
              <AppButton variant="ghost" size="sm" class="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300" on:click={() => openEditModal(unit)} ariaLabel="تعديل">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"/>
                </svg>
                <span class="text-sm mr-1">تعديل</span>
              </AppButton>

              <AppButton variant="ghost" size="sm" class="text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300" on:click={() => openDeleteModal(unit)} ariaLabel="حذف">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                </svg>
                <span class="text-sm mr-1">حذف</span>
              </AppButton>

              <AppButton variant="ghost" size="sm" class="text-civil-blue hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300" on:click={() => handleExportPackage(unit)} ariaLabel="تصدير حزمة التكوين (.unit)">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
                </svg>
                <span class="text-sm mr-1">تصدير</span>
              </AppButton>
            </td>
          </tr>
        {/each}
      </AppTable>
    </AppCard>
  </div>
</Layout>

<!-- Create Modal -->
<AppDialog open={showModal} title="وحدة جديدة" on:close={closeModal}>
  <div dir="rtl" class="space-y-4">
    {#if error}
      <AppAlert intent="danger" dismissible on:dismiss={() => error = ''}>{error}</AppAlert>
    {/if}

    <AppInput id="unitCode" label="رمز الوحدة *" placeholder="مثال: U001" bind:value={unitCode} />
    <AppInput id="unitName" label="اسم الوحدة *" placeholder="مثال: الوحدة المتنقلة الجزائر" bind:value={unitName} />

    <div class="border-t border-gray-200 dark:border-gray-700 pt-4 mt-2">
      <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-2">بيانات تسجيل الدخول</h3>
      <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">ستستخدم هذه البيانات من قبل الوحدة للاتصال بالنظام.</p>

      <AppInput id="username" label="اسم المستخدم *" placeholder="اسم المستخدم" bind:value={username} class="mb-4" />
      
      <div class="grid grid-cols-2 gap-4">
        <AppInput id="password" label="كلمة المرور *" type="password" placeholder="••••••••" bind:value={password} />
        <AppInput id="confirmPassword" label="تأكيد *" type="password" placeholder="تأكيد" bind:value={confirmPassword} />
      </div>
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="primary" on:click={saveUnit}>إنشاء</AppButton>
  </svelte:fragment>
</AppDialog>

<!-- Edit Modal -->
<AppDialog open={showEditModal} title="تعديل الوحدة" on:close={closeModal}>
  <div dir="rtl" class="space-y-4">
    {#if error}
      <AppAlert intent="danger" dismissible on:dismiss={() => error = ''}>{error}</AppAlert>
    {/if}

    <AppInput id="editUnitCode" label="رمز الوحدة *" placeholder="مثال: U001" bind:value={unitCode} />
    <AppInput id="editUnitName" label="اسم الوحدة *" placeholder="مثال: الوحدة المتنقلة الجزائر" bind:value={unitName} />

    <div class="border-t border-gray-200 dark:border-gray-700 pt-4 mt-2">
      <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-2">بيانات تسجيل الدخول (اختياري)</h3>
      <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">اترك الحقول فارغة إذا لم تريد تغيير بيانات تسجيل الدخول.</p>

      <AppInput id="editUsername" label="اسم المستخدم" placeholder="اسم المستخدم الجديد" bind:value={username} class="mb-4" />
      
      <div class="grid grid-cols-2 gap-4">
        <AppInput id="editPassword" label="كلمة المرور" type="password" placeholder="كلمة مرور جديدة" bind:value={password} />
        <AppInput id="editConfirmPassword" label="تأكيد" type="password" placeholder="تأكيد كلمة المرور" bind:value={confirmPassword} />
      </div>
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="primary" on:click={updateUnitData}>تحديث</AppButton>
  </svelte:fragment>
</AppDialog>

<!-- Delete Modal -->
<AppDialog open={showDeleteModal} title="حذف الوحدة" destructive on:close={closeModal}>
  <div dir="rtl">
    {#if error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => error = ''}>{error}</AppAlert>
      </div>
    {/if}

    <div class="text-center py-4">
      <svg class="w-12 h-12 mx-auto mb-4 text-red-500 dark:text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
      </svg>
      <h3 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-2">تأكيد الحذف</h3>
      <p class="text-gray-600 dark:text-gray-400">
        هل أنت متأكد من حذف الوحدة <span class="font-semibold text-gray-900 dark:text-white">{deletingUnit?.code}</span>؟<br />
        هذا الإجراء لا يمكن التراجع عنه.
      </p>
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="danger" on:click={deleteUnitData}>حذف</AppButton>
  </svelte:fragment>
</AppDialog>
