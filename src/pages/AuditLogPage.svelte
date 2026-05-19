<script lang="ts">
  import { onMount } from 'svelte';
  import { getAuditLog, getAuditStats, exportAuditLogExcel, cleanupAuditLogs, getSettings, saveFile, showAsk } from '../lib/tauri';
  import type { AuditEntry, AuditFilters, AuditStats, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { formatErrorMessage } from '../lib/errors';
  import { push } from 'svelte-spa-router';
  import { showSuccess, showError } from '../lib/notifications';
  import { currentUser } from '../lib/session';
  import { get } from 'svelte/store';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';
  
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppSelect from '../lib/components/ui/AppSelect.svelte';
  
  const auditOp = createOperation();
  const loading = auditOp.loading;
  const error = auditOp.error;

  const exportOp = createOperationGuard();
  const exportLoading = exportOp.loading;

  const cleanupOp = createOperationGuard();
  const cleanupLoading = cleanupOp.loading;

  // State
  let entries: AuditEntry[] = [];
  let totalCount = 0;
  let page = 0;
  const pageSize = 50;
  let hasMore = false;
  let stats: AuditStats | null = null;
  let settings: Settings | null = null;
  
  // Filters
  let filters: AuditFilters = {
    user_id: undefined,
    action: undefined,
    entity_type: undefined,
    start_date: undefined,
    end_date: undefined,
    status: undefined,
    search: undefined,
  };
  
  // Modal state
  let selectedEntry: AuditEntry | null = null;
  let showModal = false;
  
  // Date range for stats (last 30 days)
  const today = new Date();
  const thirtyDaysAgo = new Date(today);
  thirtyDaysAgo.setDate(today.getDate() - 30);
  
  const formatDate = (date: Date) => date.toISOString().split('T')[0];
  
  // Load audit log
  async function loadAuditLog(resetPage = true) {
    if (resetPage) page = 0;
    
    await auditOp.run(async () => {
      const response = await getAuditLog(filters, page, pageSize);
      entries = resetPage ? response.entries : [...entries, ...response.entries];
      totalCount = response.total_count;
      hasMore = response.has_more;
    });
  }
  
  // Load stats
  async function loadStats() {
    try {
      stats = await getAuditStats(formatDate(thirtyDaysAgo), formatDate(today));
    } catch (e) {
      // Failed to load stats
    }
  }
  
  // Format timestamp for display
  function formatTimestamp(timestamp: string): string {
    const date = new Date(timestamp);
    const options: Intl.DateTimeFormatOptions = {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    };
    return date.toLocaleDateString('ar-DZ', options);
  }
  
  // Handle page change
  function nextPage() {
    if (hasMore) {
      page++;
      loadAuditLog(false);
    }
  }
  
  function prevPage() {
    if (page > 0) {
      page--;
      loadAuditLog(false);
    }
  }
  
  // Clear filters
  function clearFilters() {
    filters = {
      user_id: undefined,
      action: undefined,
      entity_type: undefined,
      start_date: undefined,
      end_date: undefined,
      status: undefined,
      search: undefined,
    };
    loadAuditLog(true);
  }
  
  // Export to Excel
  async function exportToExcel() {
    await exportOp.guard(async () => {
      try {
        const filePath = await saveFile({
          filters: [{ name: 'Excel', extensions: ['xlsx'] }],
          defaultPath: 'audit_log.xlsx',
        });
        
        if (filePath) {
          const result = await exportAuditLogExcel(filters, filePath);
          showSuccess(`تم تصدير ${result.record_count} سجل بنجاح إلى ملف Excel`);
        }
      } catch (e) {
        showError('فشل تصدير البيانات: ' + formatErrorMessage(e));
      }
    });
  }
  
  // Cleanup old logs
  async function cleanupOldLogs() {
    const confirmed = await showAsk('هل أنت متأكد من حذف السجلات القديمة (أكثر من سنة)؟', {
      title: 'تأكيد الحذف',
      kind: 'warning',
      okLabel: 'نعم',
      cancelLabel: 'لا',
    });
    if (!confirmed) return;
    
    await cleanupOp.guard(async () => {
      try {
        const deleted = await cleanupAuditLogs();
        showSuccess(`تم حذف ${deleted} سجل قديم`);
        loadAuditLog(true);
        loadStats();
      } catch (e) {
        showError('فشل حذف السجلات: ' + formatErrorMessage(e));
      }
    });
  }
  
  // Show entry details
  function showDetails(entry: AuditEntry) {
    selectedEntry = entry;
    showModal = true;
  }
  
  // Close modal
  function closeModal() {
    showModal = false;
    selectedEntry = null;
  }
  
  // Load settings and data on mount
  onMount(async () => {
    try {
      settings = await getSettings();
      
      const user = get(currentUser);
      if (user) {
        // Authorization Guard: 
        // - WILAYA: All users
        // - UNIT: Only Admin
        if (settings.node_type === 'UNIT' && user.role !== 'Admin') {
          push('/unit');
          return;
        }
      }
    } catch (e) {
      // Failed to load settings
    }
    loadAuditLog();
    loadStats();
  });

  $: nodeType = (settings?.node_type as 'WILAYA' | 'UNIT' | null) || null;

  const actionOptions = [
    { value: undefined, label: 'الكل' },
    { value: 'Login', label: 'تسجيل دخول' },
    { value: 'CreateProduct', label: 'إنشاء منتج' },
    { value: 'UpdateProduct', label: 'تعديل منتج' },
    { value: 'DeleteProduct', label: 'حذف منتج' },
    { value: 'ConfirmOrder', label: 'تأكيد طلبية' },
    { value: 'CreateDailyReport', label: 'إنشاء تقرير يومي' },
    { value: 'CreateUnit', label: 'إنشاء وحدة' },
    { value: 'UpdateUnit', label: 'تعديل وحدة' },
    { value: 'PasswordChange', label: 'تغيير كلمة المرور' },
    { value: 'RestoreBackup', label: 'استعادة نسخة احتياطية' }
  ];

  const statusOptions = [
    { value: undefined, label: 'الكل' },
    { value: 'Success', label: 'ناجح' },
    { value: 'Failed', label: 'فاشل' }
  ];
</script>

<Layout {nodeType} title="سجل التدقيق" subtitle="متابعة ومراقبة جميع العمليات في النظام">
  <div class="max-w-7xl mx-auto" dir="rtl">
    <!-- Header -->
    <AppPageHeader title="سجل التدقيق">
      <svelte:fragment slot="actions">
        <AppButton variant="secondary" on:click={exportToExcel} loading={$exportLoading} disabled={$loading || $exportLoading || $cleanupLoading}>
          <svg class="w-5 h-5 mr-1 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
          </svg>
          تصدير Excel
        </AppButton>
        <AppButton variant="danger" on:click={cleanupOldLogs} loading={$cleanupLoading} disabled={$loading || $exportLoading || $cleanupLoading}>
          <svg class="w-5 h-5 mr-1 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
          </svg>
          تنظيف السجلات القديمة
        </AppButton>
      </svelte:fragment>
    </AppPageHeader>

    <!-- Statistics Cards -->
    {#if stats}
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
        <AppCard padding="sm">
          <div class="text-sm text-gray-500 dark:text-gray-400">إجمالي العمليات (30 يوم)</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-gray-100">{stats.total_operations.toLocaleString()}</div>
        </AppCard>
        <AppCard padding="sm">
          <div class="text-sm text-gray-500 dark:text-gray-400">العمليات الناجحة</div>
          <div class="text-2xl font-bold text-green-600">
            {(stats.total_operations - stats.failed_operations).toLocaleString()}
          </div>
        </AppCard>
        <AppCard padding="sm">
          <div class="text-sm text-gray-500 dark:text-gray-400">العمليات الفاشلة</div>
          <div class="text-2xl font-bold text-red-600">{stats.failed_operations.toLocaleString()}</div>
        </AppCard>
        <AppCard padding="sm">
          <div class="text-sm text-gray-500 dark:text-gray-400">نسبة النجاح</div>
          <div class="text-2xl font-bold text-blue-600">{stats.success_rate.toFixed(1)}%</div>
        </AppCard>
      </div>
    {/if}

    <!-- Filters -->
    <AppCard padding="md" class="mb-6">
      <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-6 gap-4">
        <div>
          <AppInput
            id="search-user"
            label="المستخدم"
            bind:value={filters.search}
            placeholder="بحث في اسم المستخدم..."
          />
        </div>
        
        <div>
          <AppSelect
            id="filter-action"
            label="نوع العملية"
            bind:value={filters.action}
          >
            {#each actionOptions as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </AppSelect>
        </div>
        
        <div>
          <AppSelect
            id="filter-status"
            label="الحالة"
            bind:value={filters.status}
          >
            {#each statusOptions as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </AppSelect>
        </div>
        
        <div>
          <AppInput
            id="filter-start-date"
            type="date"
            label="من تاريخ"
            bind:value={filters.start_date}
          />
        </div>
        
        <div>
          <AppInput
            id="filter-end-date"
            type="date"
            label="إلى تاريخ"
            bind:value={filters.end_date}
          />
        </div>
        
        <div class="flex items-end gap-2">
          <div class="flex-1">
            <AppButton variant="primary" fullWidth on:click={() => loadAuditLog(true)} disabled={$loading || $exportLoading || $cleanupLoading}>
              بحث
            </AppButton>
          </div>
          <div>
            <AppButton variant="secondary" on:click={clearFilters} disabled={$loading || $exportLoading || $cleanupLoading}>
              مسح
            </AppButton>
          </div>
        </div>
      </div>
    </AppCard>

    <!-- Error Message -->
    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => auditOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    <!-- Table -->
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <AppTable
        loading={$loading}
        empty={!$loading && entries.length === 0}
        emptyMessage="لا توجد نتائج"
      >
        <svelte:fragment slot="head">
          <th class="table-header">الوقت</th>
          <th class="table-header">المستخدم</th>
          <th class="table-header">العملية</th>
          <th class="table-header">نوع الكيان</th>
          <th class="table-header">الكيان</th>
          <th class="table-header">الحالة</th>
          <th class="table-header">تفاصيل</th>
        </svelte:fragment>

        {#each entries as entry}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
            <td class="table-cell">{formatTimestamp(entry.timestamp)}</td>
            <td class="table-cell font-medium">{entry.username}</td>
            <td class="table-cell">{entry.action_display}</td>
            <td class="table-cell">{entry.entity_type_display}</td>
            <td class="table-cell">{entry.entity_name || '-'}</td>
            <td class="table-cell">
              <AppBadge intent={entry.status === 'Success' ? 'success' : 'danger'}>
                {entry.status === 'Success' ? 'ناجح' : 'فاشل'}
              </AppBadge>
            </td>
            <td class="table-cell">
              <AppButton variant="ghost" size="sm" on:click={() => showDetails(entry)}>عرض</AppButton>
            </td>
          </tr>
        {/each}
      </AppTable>

      <!-- Pagination -->
      <div class="bg-gray-50 dark:bg-gray-900 px-4 py-3 flex items-center justify-between border-t border-gray-200 dark:border-gray-700">
        <div class="text-sm text-gray-700 dark:text-gray-100">
          الصفحة {page + 1} من {Math.ceil(totalCount / pageSize) || 1}
          <span class="mx-2">|</span>
          إجمالي: {totalCount} نتيجة
        </div>
        <div class="flex gap-2">
          <AppButton
            variant="secondary"
            size="sm"
            disabled={page === 0 || $loading}
            on:click={prevPage}
          >
            السابق
          </AppButton>
          <AppButton
            variant="secondary"
            size="sm"
            disabled={!hasMore || $loading}
            on:click={nextPage}
          >
            التالي
          </AppButton>
        </div>
      </div>
    </div>
  </div>
</Layout>

<!-- Detail Modal -->
<AppDialog
  open={showModal && selectedEntry !== null}
  title="تفاصيل العملية"
  size="lg"
  on:close={closeModal}
>
  {#if selectedEntry}
    <div class="space-y-4 text-sm" dir="rtl">
      <div class="grid grid-cols-2 gap-4">
        <div>
          <span class="text-gray-500 dark:text-gray-400">المعرف:</span>
          <span class="text-gray-900 dark:text-gray-100 font-mono block">{selectedEntry.id}</span>
        </div>
        <div>
          <span class="text-gray-500 dark:text-gray-400">الوقت:</span>
          <span class="text-gray-900 dark:text-gray-100 block">{formatTimestamp(selectedEntry.timestamp)}</span>
        </div>
        <div>
          <span class="text-gray-500 dark:text-gray-400">المستخدم:</span>
          <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.username}</span>
        </div>
        <div>
          <span class="text-gray-500 dark:text-gray-400">الحالة:</span>
          <span class="block">
            <AppBadge intent={selectedEntry.status === 'Success' ? 'success' : 'danger'}>
              {selectedEntry.status === 'Success' ? 'ناجح' : 'فاشل'}
            </AppBadge>
          </span>
        </div>
      </div>

      <div>
        <span class="text-gray-500 dark:text-gray-400">العملية:</span>
        <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.action_display}</span>
      </div>

      <div>
        <span class="text-gray-500 dark:text-gray-400">نوع الكيان:</span>
        <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.entity_type_display}</span>
      </div>

      {#if selectedEntry.entity_name}
        <div>
          <span class="text-gray-500 dark:text-gray-400">اسم الكيان:</span>
          <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.entity_name}</span>
        </div>
      {/if}

      {#if selectedEntry.old_value}
        <div>
          <span class="text-gray-500 dark:text-gray-400">القيمة القديمة:</span>
          <pre class="mt-1 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 p-3 rounded-lg text-xs overflow-auto font-mono text-gray-800 dark:text-gray-200">{JSON.stringify(selectedEntry.old_value, null, 2)}</pre>
        </div>
      {/if}

      {#if selectedEntry.new_value}
        <div>
          <span class="text-gray-500 dark:text-gray-400">القيمة الجديدة:</span>
          <pre class="mt-1 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 p-3 rounded-lg text-xs overflow-auto font-mono text-gray-800 dark:text-gray-200">{JSON.stringify(selectedEntry.new_value, null, 2)}</pre>
        </div>
      {/if}

      {#if selectedEntry.error_message}
        <div>
          <span class="text-gray-500 dark:text-gray-400">رسالة الخطأ:</span>
          <div class="mt-1">
            <AppAlert intent="danger">{selectedEntry.error_message}</AppAlert>
          </div>
        </div>
      {/if}

      {#if selectedEntry.metadata}
        <div>
          <span class="text-gray-500 dark:text-gray-400">البيانات الإضافية:</span>
          <pre class="mt-1 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 p-3 rounded-lg text-xs overflow-auto font-mono text-gray-800 dark:text-gray-200">{JSON.stringify(selectedEntry.metadata, null, 2)}</pre>
        </div>
      {/if}
    </div>
  {/if}
  
  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={closeModal}>إغلاق</AppButton>
  </svelte:fragment>
</AppDialog>
