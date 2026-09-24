<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { formatErrorMessage } from "../lib/errors";
  import { saveFile } from "../lib/tauri";
  import { exportUnitNodePackage } from "../lib/contracts";
  import { listUnits, createUnit, getSettings, updateUnit, deleteUnit } from "../lib/contracts";
  import type { Unit, Settings, CreateUnitRequest } from "../lib/types";
  import Layout from "../components/Layout.svelte";
  import { createOperation } from "../lib/operationGuard";
  import { createRuntimeScope, createTransientMessage } from "../lib/runtimeCleanup";

  import AppButton from "../lib/components/ui/AppButton.svelte";
  import AppAlert from "../lib/components/ui/AppAlert.svelte";
  import AppCard from "../lib/components/ui/AppCard.svelte";
  import AppInput from "../lib/components/ui/AppInput.svelte";
  import AppTable from "../lib/components/ui/AppTable.svelte";
  import AppDialog from "../lib/components/ui/AppDialog.svelte";
  import AppPageHeader from "../lib/components/ui/AppPageHeader.svelte";
  import AppEmptyState from "../lib/components/ui/AppEmptyState.svelte";

  const scope = createRuntimeScope();
  const unitsOp = createOperation({ scope });
  const loading = unitsOp.loading;
  const error = unitsOp.error;

  // @category ProjectionState
  let units: Unit[] = [];
  // @category ProjectionState
  let settings: Settings | null = null;
  // @category UiState
  let showModal = false;
  // @category UiState
  let showEditModal = false;
  // @category UiState
  let showDeleteModal = false;
  // @category TransientState
  let success = "";
  // @category UiState
  let editingUnit: Unit | null = null;
  // @category UiState
  let deletingUnit: Unit | null = null;

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m));
  onDestroy(() => scope.dispose());

  // Form fields
  // @category TransientState
  let unitCode = "";
  // @category TransientState
  let unitName = "";
  // @category TransientState
  let password = "";
  // @category TransientState
  let confirmPassword = "";

  onMount(async () => {
    loadData();
  });

  async function loadData() {
    await unitsOp.run(async () => {
      settings = await getSettings();
      if (settings && settings.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
      }
    });
  }

  function openCreateModal() {
    unitCode = "";
    unitName = "";
    password = "";
    confirmPassword = "";
    showModal = true;
    unitsOp.error.set(null);
  }

  function openEditModal(unit: Unit) {
    editingUnit = unit;
    unitCode = unit.code;
    unitName = unit.name;
    password = "";
    confirmPassword = "";
    showEditModal = true;
    unitsOp.error.set(null);
  }

  function openDeleteModal(unit: Unit) {
    deletingUnit = unit;
    showDeleteModal = true;
    unitsOp.error.set(null);
  }

  function closeModal() {
    showModal = false;
    showEditModal = false;
    showDeleteModal = false;
    editingUnit = null;
    deletingUnit = null;
    unitsOp.error.set(null);
  }

  async function saveUnit() {
    if (!unitCode || !unitName || !password) {
      unitsOp.error.set("الرجاء إدخال رمز واسم الوحدة");
      return;
    }

    if (password !== confirmPassword) {
      unitsOp.error.set("كلمات المرور غير متطابقة");
      return;
    }

    if (!settings?.wilaya_code) {
      unitsOp.error.set("تكوين الولاية غير موجود");
      return;
    }

    try {
      const request: CreateUnitRequest = {
        code: unitCode,
        name: unitName,
        password,
      };
      await createUnit(request, settings.wilaya_code);
      setSuccessWithTimeout(`تم إنشاء الوحدة ${unitCode} بنجاح.`);
      closeModal();
      loadData();
    } catch (e) {
      unitsOp.error.set("خطأ: " + formatErrorMessage(e));
    }
  }

  async function updateUnitData() {
    if (!unitCode || !unitName) {
      unitsOp.error.set("الرجاء إدخال رمز واسم الوحدة");
      return;
    }

    if (!editingUnit) {
      unitsOp.error.set("لم يتم تحديد الوحدة للتعديل");
      return;
    }

    try {
      const request: CreateUnitRequest = {
        code: unitCode,
        name: unitName,
        password: password || "",
      };
      await updateUnit(editingUnit.id, request);
      setSuccessWithTimeout(`تم تحديث الوحدة ${unitCode} بنجاح.`);
      closeModal();
      loadData();
    } catch (e) {
      unitsOp.error.set("خطأ: " + formatErrorMessage(e));
    }
  }

  async function deleteUnitData() {
    if (!deletingUnit) {
      unitsOp.error.set("لم يتم تحديد الوحدة للحذف");
      return;
    }

    try {
      await deleteUnit(deletingUnit.id);
      setSuccessWithTimeout(`تم حذف الوحدة ${deletingUnit.code} بنجاح.`);
      closeModal();
      loadData();
    } catch (e) {
      unitsOp.error.set("خطأ: " + formatErrorMessage(e));
    }
  }

  async function handleExportPackage(unit: Unit) {
    const filePath = await saveFile({
      filters: [
        {
          name: "حزمة التكوين",
          extensions: ["unit"],
        },
      ],
      defaultPath: `${unit.code}_package.unit`,
    });

    if (filePath) {
      await unitsOp.run(async () => {
        const result = await exportUnitNodePackage(unit.id, filePath);
        if (result.success) {
          setSuccessWithTimeout(`تم تصدير حزمة الوحدة ${unit.code} بنجاح.`);
        } else {
          throw new Error(result.message || "فشل تصدير الحزمة");
        }
      });
    }
  }
</script>

<Layout nodeType="WILAYA" title="إدارة الوحدات" subtitle="إنشاء وتكوين الوحدات">
  <div dir="rtl">
    <AppPageHeader title="إدارة الوحدات" subtitle="إنشاء وتكوين الوحدات">
      <svelte:fragment slot="actions">
        <AppButton variant="primary" on:click={openCreateModal}>
          <svg
            class="w-4 h-4 mr-2 inline-block"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M12 4v16m8-8H4"
            />
          </svg>
          وحدة جديدة
        </AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => unitsOp.error.set(null)}
          >{$error}</AppAlert
        >
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => (success = "")}
          >{success}</AppAlert
        >
      </div>
    {/if}

    <AppCard padding="none">
      <AppTable loading={$loading} empty={units.length === 0}>
        <svelte:fragment slot="empty">
          <AppEmptyState
            title="لا يوجد وحدات مسجلة"
            description="أنشئ وحدتك الأولى للبدء في المزامنة."
            icon="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
          >
            <svelte:fragment slot="action">
              <AppButton variant="primary" on:click={openCreateModal}
                >إنشاء أول وحدة</AppButton
              >
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
          <tr
            class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
          >
            <td class="table-cell font-medium">{unit.code}</td>
            <td class="table-cell">{unit.name}</td>
            <td class="table-cell"
              >{new Date(unit.created_at).toLocaleDateString("ar-EG")}</td
            >
            <td class="table-cell text-left space-x-2 space-x-reverse">
              <AppButton
                variant="ghost"
                size="sm"
                class="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300"
                on:click={() => openEditModal(unit)}
                ariaLabel="تعديل"
              >
                <svg
                  class="w-4 h-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                  />
                </svg>
                <span class="text-sm mr-1">تعديل</span>
              </AppButton>

              <AppButton
                variant="ghost"
                size="sm"
                class="text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300"
                on:click={() => openDeleteModal(unit)}
                ariaLabel="حذف"
              >
                <svg
                  class="w-4 h-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                  />
                </svg>
                <span class="text-sm mr-1">حذف</span>
              </AppButton>

              <AppButton
                variant="ghost"
                size="sm"
                class="text-civil-blue hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300"
                on:click={() => handleExportPackage(unit)}
                ariaLabel="تصدير حزمة التكوين (.unit)"
              >
                <svg
                  class="w-4 h-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                  />
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
    {#if $error}
      <AppAlert intent="danger" dismissible on:dismiss={() => unitsOp.error.set(null)}
        >{$error}</AppAlert
      >
    {/if}

    <AppInput
      id="unitCode"
      label="رمز الوحدة *"
      placeholder="مثال: UNIT01"
      bind:value={unitCode}
    />
    <AppInput
      id="unitName"
      label="اسم الوحدة *"
      placeholder="مثال: الوحدة الرئيسية الجزائر"
      bind:value={unitName}
    />

    <div class="border-t border-gray-200 dark:border-gray-700 pt-4 mt-2">
      <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-2">
        بيانات تسجيل الدخول
      </h3>
      <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
        ستستخدم هذه البيانات من قبل الوحدة للاتصال بالنظام.
      </p>

      <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
        اسم مستخدم مشغّل الوحدة ثابت: <span class="font-semibold" dir="ltr">user</span>
      </p>

      <div class="grid grid-cols-2 gap-4">
        <AppInput
          id="password"
          label="كلمة المرور *"
          type="password"
          placeholder="••••••••"
          bind:value={password}
        />
        <AppInput
          id="confirmPassword"
          label="تأكيد *"
          type="password"
          placeholder="تأكيد"
          bind:value={confirmPassword}
        />
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
    {#if $error}
      <AppAlert intent="danger" dismissible on:dismiss={() => unitsOp.error.set(null)}
        >{$error}</AppAlert
      >
    {/if}

    <AppInput
      id="editUnitCode"
      label="رمز الوحدة *"
      placeholder="مثال: UNIT01"
      bind:value={unitCode}
    />
    <AppInput
      id="editUnitName"
      label="اسم الوحدة *"
      placeholder="مثال: الوحدة الرئيسية الجزائر"
      bind:value={unitName}
    />

    <div class="border-t border-gray-200 dark:border-gray-700 pt-4 mt-2">
      <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-2">
        بيانات تسجيل الدخول (اختياري)
      </h3>
      <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
        اترك الحقول فارغة إذا لم تريد تغيير بيانات تسجيل الدخول.
      </p>

      <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
        اسم المستخدم ثابت ولا يمكن تغييره: <span class="font-semibold" dir="ltr">user</span>
      </p>
      <div class="grid grid-cols-2 gap-4">
        <AppInput
          id="editPassword"
          label="كلمة المرور"
          type="password"
          placeholder="كلمة مرور جديدة"
          bind:value={password}
        />
        <AppInput
          id="editConfirmPassword"
          label="تأكيد"
          type="password"
          placeholder="تأكيد كلمة المرور"
          bind:value={confirmPassword}
        />
      </div>
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="primary" on:click={updateUnitData}>تحديث</AppButton>
  </svelte:fragment>
</AppDialog>

<!-- Delete Modal -->
<AppDialog
  open={showDeleteModal}
  title="حذف الوحدة"
  destructive
  on:close={closeModal}
>
  <div dir="rtl">
    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => unitsOp.error.set(null)}
          >{$error}</AppAlert
        >
      </div>
    {/if}

    <div class="text-center py-4">
      <svg
        class="w-12 h-12 mx-auto mb-4 text-red-500 dark:text-red-400"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
        />
      </svg>
      <h3 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-2">
        تأكيد الحذف
      </h3>
      <p class="text-gray-600 dark:text-gray-400">
        هل أنت متأكد من حذف الوحدة <span
          class="font-semibold text-gray-900 dark:text-white"
          >{deletingUnit?.code}</span
        >؟<br />
        هذا الإجراء لا يمكن التراجع عنه.
      </p>
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إلغاء</AppButton>
    <AppButton variant="danger" on:click={deleteUnitData}>حذف</AppButton>
  </svelte:fragment>
</AppDialog>
