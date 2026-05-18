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
  <div class="mb-8">
    <div class="flex items-center justify-end">
      <button
        on:click={openCreateModal}
        class="btn-primary flex items-center gap-2"
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
            d="M12 4v16m8-8H4"
          />
        </svg>
        <span>وحدة جديدة</span>
      </button>
    </div>
  </div>

  {#if error}
    <div
      class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm"
    >
      {error}
    </div>
  {/if}

  {#if success}
    <div
      class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm"
    >
      {success}
    </div>
  {/if}

  <div class="card">
    {#if loading}
      <div class="flex items-center justify-center py-12">
        <div
          class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"
        ></div>
      </div>
    {:else if units.length === 0}
      <div class="text-center py-12 text-gray-500 dark:text-gray-400">
        <svg
          class="w-16 h-16 mx-auto mb-4 text-gray-300"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
          />
        </svg>
        <p class="text-lg">لا يوجد وحدات مسجلة</p>
        <button
          on:click={openCreateModal}
          class="text-civil-blue hover:underline mt-2"
        >
          إنشاء أول وحدة
        </button>
      </div>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full">
          <thead>
            <tr>
              <th class="table-header">الرمز</th>
              <th class="table-header">الاسم</th>
              <th class="table-header">تاريخ الإنشاء</th>
              <th class="table-header text-right">الإجراءات</th>
            </tr>
          </thead>
          <tbody>
            {#each units as unit}
              <tr class="hover:bg-gray-50 dark:bg-gray-900">
                <td class="table-cell font-medium">{unit.code}</td>
                <td class="table-cell">{unit.name}</td>
                <td class="table-cell"
                  >{new Date(unit.created_at).toLocaleDateString("ar-EG")}</td
                >
                <td class="table-cell text-right">
                  <div class="flex items-center gap-2 justify-end">
                    <button
                      on:click={() => openEditModal(unit)}
                      class="text-blue-600 hover:text-blue-800 dark:text-blue-300 flex items-center gap-1"
                      title="تعديل الوحدة"
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
                      <span class="text-sm">تعديل</span>
                    </button>

                    <button
                      on:click={() => openDeleteModal(unit)}
                      class="text-red-600 hover:text-red-800 flex items-center gap-1"
                      title="حذف الوحدة"
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
                      <span class="text-sm">حذف</span>
                    </button>

                    <button
                      on:click={() => handleExportPackage(unit)}
                      class="text-civil-blue hover:text-civil-blue-dark flex items-center gap-1"
                      title="تصدير حزمة التكوين (.unit)"
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
                      <span class="text-sm">تصدير</span>
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>

  <!-- Create Modal -->
  {#if showModal}
    <div
      class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
    >
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-md mx-4">
        <div class="p-6 border-b border-gray-100 dark:border-gray-700">
          <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">وحدة جديدة</h2>
        </div>

        <div class="p-6 space-y-4">
          {#if error}
            <div
              class="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm"
            >
              {error}
            </div>
          {/if}

          <div>
            <label
              for="unitCode"
              class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
              >رمز الوحدة *</label
            >
            <input
              id="unitCode"
              type="text"
              class="input-field"
              placeholder="مثال: U001"
              bind:value={unitCode}
            />
          </div>

          <div>
            <label
              for="unitName"
              class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
              >اسم الوحدة *</label
            >
            <input
              id="unitName"
              type="text"
              class="input-field"
              placeholder="مثال: الوحدة المتنقلة الجزائر"
              bind:value={unitName}
            />
          </div>

          <div class="border-t border-gray-200 dark:border-gray-700 pt-4">
            <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-3">
              بيانات تسجيل الدخول
            </h3>
            <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
              ستستخدم هذه البيانات من قبل الوحدة للاتصال بالنظام.
            </p>

            <div>
              <label
                for="username"
                class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
                >اسم المستخدم *</label
              >
              <input
                id="username"
                type="text"
                class="input-field"
                placeholder="اسم المستخدم"
                bind:value={username}
              />
            </div>

            <div class="grid grid-cols-2 gap-4 mt-3">
              <div>
                <label
                  for="password"
                  class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
                  >كلمة المرور *</label
                >
                <input
                  id="password"
                  type="password"
                  class="input-field"
                  placeholder="••••••••"
                  bind:value={password}
                />
              </div>
              <div>
                <label
                  for="confirmPassword"
                  class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
                  >تأكيد *</label
                >
                <input
                  id="confirmPassword"
                  type="password"
                  class="input-field"
                  placeholder="تأكيد"
                  bind:value={confirmPassword}
                />
              </div>
            </div>
          </div>
        </div>

        <div class="p-6 border-t border-gray-100 dark:border-gray-700 flex justify-end gap-3">
          <button on:click={closeModal} class="btn-secondary"> إلغاء </button>
          <button on:click={saveUnit} class="btn-primary"> إنشاء </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Edit Modal -->
  {#if showEditModal}
    <div
      class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
    >
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-md mx-4">
        <div class="p-6 border-b border-gray-100 dark:border-gray-700">
          <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">تعديل الوحدة</h2>
        </div>

        <div class="p-6 space-y-4">
          {#if error}
            <div
              class="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm"
            >
              {error}
            </div>
          {/if}

          <div>
            <label
              for="editUnitCode"
              class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
              >رمز الوحدة *</label
            >
            <input
              id="editUnitCode"
              type="text"
              class="input-field"
              placeholder="مثال: U001"
              bind:value={unitCode}
            />
          </div>

          <div>
            <label
              for="editUnitName"
              class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
              >اسم الوحدة *</label
            >
            <input
              id="editUnitName"
              type="text"
              class="input-field"
              placeholder="مثال: الوحدة المتنقلة الجزائر"
              bind:value={unitName}
            />
          </div>

          <div class="border-t border-gray-200 dark:border-gray-700 pt-4">
            <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-3">
              بيانات تسجيل الدخول (اختياري)
            </h3>
            <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
              اترك الحقول فارغة إذا لم تريد تغيير بيانات تسجيل الدخول.
            </p>

            <div>
              <label
                for="editUsername"
                class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
                >اسم المستخدم</label
              >
              <input
                id="editUsername"
                type="text"
                class="input-field"
                placeholder="اسم المستخدم الجديد"
                bind:value={username}
              />
            </div>

            <div class="grid grid-cols-2 gap-4 mt-3">
              <div>
                <label
                  for="editPassword"
                  class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
                  >كلمة المرور</label
                >
                <input
                  id="editPassword"
                  type="password"
                  class="input-field"
                  placeholder="كلمة مرور جديدة"
                  bind:value={password}
                />
              </div>
              <div>
                <label
                  for="editConfirmPassword"
                  class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1"
                  >تأكيد</label
                >
                <input
                  id="editConfirmPassword"
                  type="password"
                  class="input-field"
                  placeholder="تأكيد كلمة المرور"
                  bind:value={confirmPassword}
                />
              </div>
            </div>
          </div>
        </div>

        <div class="p-6 border-t border-gray-100 dark:border-gray-700 flex justify-end gap-3">
          <button on:click={closeModal} class="btn-secondary"> إلغاء </button>
          <button on:click={updateUnitData} class="btn-primary"> تحديث </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Delete Modal -->
  {#if showDeleteModal}
    <div
      class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
    >
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-md mx-4">
        <div class="p-6 border-b border-gray-100 dark:border-gray-700">
          <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">حذف الوحدة</h2>
        </div>

        <div class="p-6">
          {#if error}
            <div
              class="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm mb-4"
            >
              {error}
            </div>
          {/if}

          <div class="text-center">
            <svg
              class="w-12 h-12 mx-auto mb-4 text-red-500"
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
            <p class="text-gray-600 dark:text-gray-400 mb-6">
              هل أنت متأكد من حذف الوحدة <span class="font-semibold"
                >{deletingUnit?.code}</span
              >؟
              <br />
              هذا الإجراء لا يمكن التراجع عنه.
            </p>
          </div>
        </div>

        <div class="p-6 border-t border-gray-100 dark:border-gray-700 flex justify-end gap-3">
          <button on:click={closeModal} class="btn-secondary"> إلغاء </button>
          <button
            on:click={deleteUnitData}
            class="bg-red-600 hover:bg-red-700 text-white px-4 py-2 rounded-lg transition-colors"
          >
            حذف
          </button>
        </div>
      </div>
    </div>
  {/if}
</Layout>
