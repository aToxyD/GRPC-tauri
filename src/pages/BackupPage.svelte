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
  import { createOperationGuard } from "../lib/operationGuard";
  import { formatErrorMessage } from "../lib/errors";
  import AppButton from "../lib/components/ui/AppButton.svelte";
  import AppAlert from "../lib/components/ui/AppAlert.svelte";
  import AppCard from "../lib/components/ui/AppCard.svelte";
  import AppLoadingState from "../lib/components/ui/AppLoadingState.svelte";
  import AppEmptyState from "../lib/components/ui/AppEmptyState.svelte";
  import AppPageHeader from "../lib/components/ui/AppPageHeader.svelte";

  const { loading: opLoading, guard } = createOperationGuard();

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
      error = formatErrorMessage(err);
    } finally {
      loading = false;
    }
  }

  async function handleCreateBackup() {
    await guard(async () => {
      try {
        loading = true;
        error = null;
        const backupPath = await createBackup();
        await loadBackups(); // Reload the list
        showSuccess("تم إنشاء نسخة احتياطية بنجاح");
      } catch (err) {
        error = formatErrorMessage(err);
      } finally {
        loading = false;
      }
    });
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

    await guard(async () => {
      try {
        restoring = true;
        error = null;
        const { token } = await issueOperationExecutionToken({ operation: 'restore' });
        await restoreBackup(backupPath, typed.trim(), token);
      } catch (err) {
        error = formatErrorMessage(err);
      } finally {
        restoring = false;
      }
    });
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
    <div class="mx-auto max-w-[1200px] p-6" dir="rtl">
      <AppPageHeader title="إدارة النسخ الاحتياطية">
        <svelte:fragment slot="actions">
          <AppButton
            variant="primary"
            loading={loading || $opLoading}
            disabled={$opLoading}
            on:click={handleCreateBackup}
          >
            إنشاء نسخة احتياطية
          </AppButton>
        </svelte:fragment>
      </AppPageHeader>

      <div class="mb-6">
        <AppAlert intent="info" title="دليل المشغل (Operator Guide):">
          <ul class="list-disc pr-5 space-y-1">
            <li><strong>قيود الاستعادة:</strong> لا يمكن استعادة نسخة احتياطية لسنة مالية تمت أرشفتها. النظام يحمي السجلات التاريخية من أي تلاعب أو تراجع.</li>
            <li><strong>التوكيد النصي:</strong> يُطلب منك كتابة "RESTORE" لتأكيد الاستعادة لأنها ستؤدي إلى إعادة تشغيل النظام وإلغاء أي تغييرات لم يتم حفظها في النسخة الاحتياطية.</li>
          </ul>
        </AppAlert>
      </div>

      {#if error}
        <div class="mb-4">
          <AppAlert intent="danger" dismissible on:dismiss={() => error = null}>
            {error}
          </AppAlert>
        </div>
      {/if}

      {#if loading && backups.length === 0}
        <AppLoadingState message="جاري تحميل النسخ الاحتياطية..." />
      {:else if backups.length === 0}
        <AppEmptyState
          title="لا توجد نسخ احتياطية"
          description="ابدأ بإنشاء نسخة احتياطية لحماية بياناتك"
          icon="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12"
        >
          <svelte:fragment slot="action">
            <AppButton variant="primary" loading={loading || $opLoading} disabled={$opLoading} on:click={handleCreateBackup}>
              إنشاء أول نسخة احتياطية
            </AppButton>
          </svelte:fragment>
        </AppEmptyState>
      {:else}
        <AppCard padding="none" class="mb-6">
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
                class="px-6 py-4 hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors"
              >
                <div class="flex items-center justify-between">
                  <div class="flex-1">
                    <div class="flex items-center gap-3">
                      <svg
                        class="w-5 h-5 text-green-600 dark:text-green-400"
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
                      <AppButton
                        variant="danger"
                        size="sm"
                        disabled={restoring || $opLoading}
                        loading={restoring}
                        on:click={() => handleRestoreBackup(backup.path)}
                      >
                        استعادة
                      </AppButton>
                    {:else}
                      <div class="text-gray-500 dark:text-gray-400 text-sm px-3 py-1 flex items-center">
                        <svg
                          class="w-4 h-4 ml-1"
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
        </AppCard>
      {/if}

      <div class="mt-8">
        <AppAlert intent="info" title="معلومات هامة:">
          <ul class="text-sm space-y-1">
            <li>• يتم الاحتفاظ بآخر 7 نسخ احتياطية فقط</li>
            <li>• يتم إنشاء نسخة احتياطية تلقائيا كل 24 ساعة</li>
            <li>• استعادة النسخة الاحتياطية تستبدل البيانات الحالية</li>
            <li>• يوصى بإنشاء نسخة احتياطية قبل أي تحديثات مهمة</li>
            {#if !isAdmin}
              <li class="font-medium">
                • استعادة النسخ الاحتياطية متاحة فقط للمسؤولين
              </li>
            {/if}
          </ul>
        </AppAlert>
      </div>
    </div>
  </Layout>
{:else}
  <AppLoadingState />
{/if}
