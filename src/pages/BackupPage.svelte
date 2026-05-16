<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import {
    createBackup,
    issueOperationExecutionToken,
    listBackups,
    restoreBackup,
    getSettings,
  } from "../lib/tauri";
  import type { BackupInfo, Settings } from "../lib/types";
  import { formatBytes, formatDate } from "../lib/utils";
  import { showSuccess } from "../lib/notifications";
  import { currentUser as userStore } from "../lib/session";
  import Layout from "../components/Layout.svelte";

  let backups: BackupInfo[] = [];
  let loading = false;
  let error: string | null = null;
  let restoring = false;
  let settings: Settings | null = null;

  $: currentUser = $userStore;

  onMount(async () => {
    await loadSettings();
    loadBackups();
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
    } catch (e) {
      // Failed to load settings
    }
  }

  $: isAdmin = currentUser?.role === "Admin";
  $: nodeType = (settings?.node_type === "WILAYA" ? "WILAYA" : "UNIT") as
    | "WILAYA"
    | "UNIT";

  async function loadBackups() {
    try {
      loading = true;
      error = null;
      backups = await listBackups();
    } catch (err) {
      error = "فشل تحميل قائمة النسخ الاحتياطية";
    } finally {
      loading = false;
    }
  }

  async function handleCreateBackup() {
    try {
      loading = true;
      error = null;
      const backupPath = await createBackup();
      await loadBackups(); // Reload the list
      showSuccess("تم إنشاء نسخة احتياطية بنجاح");
    } catch (err) {
      error = "فشل إنشاء النسخة الاحتياطية";
    } finally {
      loading = false;
    }
  }

  async function handleRestoreBackup(backupPath: string) {
    if (!isAdmin) {
      error =
        "غير مصرح لك باستعادة النسخ الاحتياطية. هذه الميزة متاحة فقط للمسؤول.";
      return;
    }

    const confirmed = await ask(
      "سيتم مسح جميع البيانات السابقة في عملية الإستعادة و إستبدالها ببيانات النسخة الجديدة، ستتم عملية الإستعادة وإعادة تشغيل التطبيق. هل تريد الإستمرار؟",
      {
        title: "تحذير: استعادة النسخة الاحتياطية",
        kind: "warning",
        okLabel: "نعم",
        cancelLabel: "لا",
      },
    );

    if (!confirmed) {
      return;
    }

    const typed = window.prompt(
      'للتأكيد اكتب RESTORE بالأحرف اللاتينية الكبيرة ثم اضغط موافق:',
      '',
    );
    if (typed?.trim() !== 'RESTORE') {
      error = 'تم إلغاء الاستعادة: التأكيد المكتوب غير صحيح.';
      return;
    }

    try {
      restoring = true;
      const { token } = await issueOperationExecutionToken({ operation: 'restore' });
      await restoreBackup(backupPath, typed.trim(), token);
    } catch (err) {
      error = "فشل استعادة النسخة الاحتياطية";
    } finally {
      restoring = false;
    }
  }

  function formatFileSize(bytes: number): string {
    const units = ["B", "KB", "MB", "GB"];
    let size = bytes;
    let unitIndex = 0;

    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }

    return `${size.toFixed(2)} ${units[unitIndex]}`;
  }

  function sortBackups() {
    backups.sort(
      (a, b) => new Date(b.created).getTime() - new Date(a.created).getTime(),
    );
  }

  $: sortBackups();
</script>

{#if settings}
  <Layout {nodeType} title="إدارة النسخ الاحتياطية">
    <div class="container mx-auto p-6">
      <div class="flex justify-between items-center mb-6">
        <h1 class="text-3xl font-bold text-gray-800 dark:text-white">
          إدارة النسخ الاحتياطية
        </h1>
        <button
          on:click={handleCreateBackup}
          disabled={loading}
          class="bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white px-4 py-2 rounded-lg flex items-center gap-2"
        >
          {#if loading}
            <div
              class="animate-spin rounded-full h-4 w-4 border-b-2 border-white"
            ></div>
          {/if}
          إنشاء نسخة احتياطية
        </button>
      </div>

      <div class="bg-blue-50 border border-blue-200 p-4 rounded-xl mb-6 text-sm text-blue-800">
        <h3 class="font-bold mb-1">دليل المشغل (Operator Guide):</h3>
        <ul class="list-disc pr-5 space-y-1">
          <li><strong>قيود الاستعادة:</strong> لا يمكن استعادة نسخة احتياطية لسنة مالية تمت أرشفتها. النظام يحمي السجلات التاريخية من أي تلاعب أو تراجع.</li>
          <li><strong>التوكيد النصي:</strong> يُطلب منك كتابة "RESTORE" لتأكيد الاستعادة لأنها ستؤدي إلى إعادة تشغيل النظام وإلغاء أي تغييرات لم يتم حفظها في النسخة الاحتياطية.</li>
        </ul>
      </div>

      {#if error}
        <div
          class="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4"
        >
          {error}
        </div>
      {/if}

      {#if loading && backups.length === 0}
        <div class="text-center py-12">
          <div
            class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"
          ></div>
          <p class="text-gray-600">جاري تحميل النسخ الاحتياطية...</p>
        </div>
      {:else if backups.length === 0}
        <div class="text-center py-12 bg-gray-50 rounded-lg">
          <svg
            class="w-16 h-16 mx-auto mb-4 text-gray-400"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"
            ></path>
          </svg>
          <h3 class="text-lg font-medium text-gray-900 mb-2">
            لا توجد نسخ احتياطية
          </h3>
          <p class="text-gray-600 mb-4">
            ابدأ بإنشاء نسخة احتياطية لحماية بياناتك
          </p>
          <button
            on:click={handleCreateBackup}
            class="bg-blue-600 hover:bg-blue-700 text-white px-6 py-2 rounded-lg"
          >
            إنشاء أول نسخة احتياطية
          </button>
        </div>
      {:else}
        <div
          class="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden"
        >
          <div class="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
            <h2 class="text-lg font-semibold text-gray-800 dark:text-white">
              النسخ الاحتياطية المتاحة
            </h2>
            <p class="text-sm text-gray-600 dark:text-gray-400 mt-1">
              آخر {backups.length} نسخة احتياطية
            </p>
          </div>

          <div class="divide-y divide-gray-200 dark:divide-gray-700">
            {#each backups as backup (backup.filename)}
              <div
                class="px-6 py-4 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
              >
                <div class="flex items-center justify-between">
                  <div class="flex-1">
                    <div class="flex items-center gap-3">
                      <svg
                        class="w-5 h-5 text-green-600"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="2"
                          d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                        ></path>
                      </svg>
                      <div>
                        <h3 class="font-medium text-gray-900 dark:text-white">
                          {backup.filename}
                        </h3>
                        <p class="text-sm text-gray-600 dark:text-gray-400">
                          {formatDate(backup.created)} • {formatFileSize(
                            backup.size_bytes,
                          )}
                        </p>
                      </div>
                    </div>
                  </div>

                  <div class="flex items-center gap-2">
                    {#if isAdmin}
                      <button
                        on:click={() => handleRestoreBackup(backup.path)}
                        disabled={restoring}
                        class="bg-orange-600 hover:bg-orange-700 disabled:bg-gray-400 text-white px-3 py-1 rounded text-sm"
                      >
                        {#if restoring}
                          <div
                            class="animate-spin rounded-full h-3 w-3 border-b-2 border-white inline-block"
                          ></div>
                        {:else}
                          استعادة
                        {/if}
                      </button>
                    {:else}
                      <div class="text-gray-500 text-sm px-3 py-1">
                        <svg
                          class="w-4 h-4 inline-block ml-1"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            stroke-width="2"
                            d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
                          />
                        </svg>
                        مقتصر على المسؤول
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="mt-8 bg-blue-50 dark:bg-blue-900/20 rounded-lg p-4">
        <h3 class="font-semibold text-blue-900 dark:text-blue-100 mb-2">
          معلومات هامة:
        </h3>
        <ul class="text-sm text-blue-800 dark:text-blue-200 space-y-1">
          <li>• يتم الاحتفاظ بآخر 7 نسخ احتياطية فقط</li>
          <li>• يتم إنشاء نسخة احتياطية تلقائيا كل 24 ساعة</li>
          <li>• استعادة النسخة الاحتياطية تستبدل البيانات الحالية</li>
          <li>• يوصى بإنشاء نسخة احتياطية قبل أي تحديثات مهمة</li>
          {#if !isAdmin}
            <li class="text-orange-600 font-medium">
              • استعادة النسخ الاحتياطية متاحة فقط للمسؤولين
            </li>
          {/if}
        </ul>
      </div>
    </div>
  </Layout>
{:else}
  <div class="flex items-center justify-center h-screen">
    <div
      class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"
    ></div>
  </div>
{/if}

<style>
  .container {
    max-width: 1200px;
  }
</style>
