<script lang="ts">
  import { onMount } from 'svelte';
  import { getAuditLog, getAuditStats, exportAuditLogExcel, cleanupAuditLogs, getSettings } from '../lib/tauri';
  import type { AuditEntry, AuditFilters, AuditStats, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { save } from '@tauri-apps/plugin-dialog';
  import { push } from 'svelte-spa-router';
  import { showSuccess, showError } from '../lib/notifications';
  import { currentUser } from '../lib/session';
  import { get } from 'svelte/store';
  
  // State
  let entries: AuditEntry[] = [];
  let totalCount = 0;
  let page = 0;
  const pageSize = 50;
  let hasMore = false;
  let loading = false;
  let error: string | null = null;
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
    loading = true;
    error = null;
    
    try {
      const response = await getAuditLog(filters, page, pageSize);
      entries = resetPage ? response.entries : [...entries, ...response.entries];
      totalCount = response.total_count;
      hasMore = response.has_more;
    } catch (e) {
      error = e instanceof Error ? e.message : 'حدث خطأ في تحميل البيانات';
    } finally {
      loading = false;
    }
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
    try {
      const filePath = await save({
        filters: [{ name: 'Excel', extensions: ['xlsx'] }],
        defaultPath: 'audit_log.xlsx',
      });
      
      if (filePath) {
        const result = await exportAuditLogExcel(filters, filePath);
        showSuccess(`تم تصدير ${result.record_count} سجل بنجاح إلى ملف Excel`);
      }
    } catch (e) {
      showError('فشل تصدير البيانات: ' + (e instanceof Error ? e.message : 'خطأ غير معروف'));
    }
  }
  
  // Cleanup old logs
  async function cleanupOldLogs() {
    if (!confirm('هل أنت متأكد من حذف السجلات القديمة (أكثر من سنة)؟')) return;
    
    try {
      const deleted = await cleanupAuditLogs();
      showSuccess(`تم حذف ${deleted} سجل قديم`);
      loadAuditLog(true);
      loadStats();
    } catch (e) {
      showError('فشل حذف السجلات: ' + (e instanceof Error ? e.message : 'خطأ غير معروف'));
    }
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
</script>

<Layout {nodeType} title="سجل التدقيق" subtitle="متابعة ومراقبة جميع العمليات في النظام">
<div class="max-w-7xl mx-auto" dir="rtl">
    <!-- Header -->
    <div class="flex justify-between items-center mb-6">
      <h1 class="text-2xl font-bold text-gray-800 dark:text-gray-100">سجل التدقيق</h1>
      <div class="flex gap-2">
        <button
          on:click={exportToExcel}
          class="px-4 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors flex items-center gap-2"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
          </svg>
          تصدير Excel
        </button>
        <button
          on:click={cleanupOldLogs}
          class="px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors flex items-center gap-2"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
          </svg>
          تنظيف السجلات القديمة
        </button>
      </div>
    </div>

    <!-- Statistics Cards -->
    {#if stats}
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div class="text-sm text-gray-500 dark:text-gray-400">إجمالي العمليات (30 يوم)</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-gray-100">{stats.total_operations.toLocaleString()}</div>
        </div>
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div class="text-sm text-gray-500 dark:text-gray-400">العمليات الناجحة</div>
          <div class="text-2xl font-bold text-green-600">
            {(stats.total_operations - stats.failed_operations).toLocaleString()}
          </div>
        </div>
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div class="text-sm text-gray-500 dark:text-gray-400">العمليات الفاشلة</div>
          <div class="text-2xl font-bold text-red-600">{stats.failed_operations.toLocaleString()}</div>
        </div>
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div class="text-sm text-gray-500 dark:text-gray-400">نسبة النجاح</div>
          <div class="text-2xl font-bold text-blue-600">{stats.success_rate.toFixed(1)}%</div>
        </div>
      </div>
    {/if}

    <!-- Filters -->
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 mb-6">
      <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-6 gap-4">
        <div>
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1" for="search-user">المستخدم</label>
          <input
            id="search-user"
            type="text"
            bind:value={filters.search}
            placeholder="بحث في اسم المستخدم..."
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
        </div>
        
        <div>
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1" for="filter-action">نوع العملية</label>
          <select
            id="filter-action"
            bind:value={filters.action}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          >
            <option value={undefined}>الكل</option>
            <option value="Login">تسجيل دخول</option>
            <option value="CreateProduct">إنشاء منتج</option>
            <option value="UpdateProduct">تعديل منتج</option>
            <option value="DeleteProduct">حذف منتج</option>
            <option value="ConfirmOrder">تأكيد طلبية</option>
            <option value="CreateDailyReport">إنشاء تقرير يومي</option>
            <option value="CreateUnit">إنشاء وحدة</option>
            <option value="UpdateUnit">تعديل وحدة</option>
            <option value="PasswordChange">تغيير كلمة المرور</option>
            <option value="RestoreBackup">استعادة نسخة احتياطية</option>
          </select>
        </div>
        
        <div>
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1" for="filter-status">الحالة</label>
          <select
            id="filter-status"
            bind:value={filters.status}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          >
            <option value={undefined}>الكل</option>
            <option value="Success">ناجح</option>
            <option value="Failed">فاشل</option>
          </select>
        </div>
        
        <div>
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1" for="filter-start-date">من تاريخ</label>
          <input
            id="filter-start-date"
            type="date"
            bind:value={filters.start_date}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
        </div>
        
        <div>
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1" for="filter-end-date">إلى تاريخ</label>
          <input
            id="filter-end-date"
            type="date"
            bind:value={filters.end_date}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
        </div>
        
        <div class="flex items-end gap-2">
          <button
            on:click={() => loadAuditLog(true)}
            class="flex-1 px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors"
          >
            بحث
          </button>
          <button
            on:click={clearFilters}
            class="px-4 py-2 bg-gray-200 text-gray-700 dark:text-gray-100 rounded-lg hover:bg-gray-300 transition-colors"
          >
            مسح
          </button>
        </div>
      </div>
    </div>

    <!-- Error Message -->
    {#if error}
      <div class="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded-lg mb-4">
        {error}
      </div>
    {/if}

    <!-- Table -->
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <div class="overflow-x-auto">
        <table class="min-w-full">
          <thead class="bg-gray-50 dark:bg-gray-900">
            <tr>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">الوقت</th>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">المستخدم</th>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">العملية</th>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">نوع الكيان</th>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">الكيان</th>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">الحالة</th>
              <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">تفاصيل</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-gray-200">
            {#each entries as entry}
              <tr class="hover:bg-gray-50 dark:bg-gray-900">
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-600 dark:text-gray-400">
                  {formatTimestamp(entry.timestamp)}
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900 dark:text-gray-100">
                  {entry.username}
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-600 dark:text-gray-400">
                  {entry.action_display}
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-600 dark:text-gray-400">
                  {entry.entity_type_display}
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-600 dark:text-gray-400">
                  {entry.entity_name || '-'}
                </td>
                <td class="px-6 py-4 whitespace-nowrap">
                  <span class="px-2 inline-flex text-xs leading-5 font-semibold rounded-full {entry.status === 'Success' ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}">
                    {entry.status === 'Success' ? 'ناجح' : 'فاشل'}
                  </span>
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm">
                  <button
                    on:click={() => showDetails(entry)}
                    class="text-blue-600 hover:text-blue-900"
                  >
                    عرض
                  </button>
                </td>
              </tr>
            {:else}
              <tr>
                <td colspan="7" class="px-6 py-8 text-center text-gray-500 dark:text-gray-400">
                  {#if loading}
                    <div class="flex justify-center items-center">
                      <svg class="animate-spin h-5 w-5 text-blue-600" fill="none" viewBox="0 0 24 24">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
                        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"/>
                      </svg>
                    </div>
                  {:else}
                    لا توجد نتائج
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <!-- Pagination -->
      <div class="bg-gray-50 dark:bg-gray-900 px-4 py-3 flex items-center justify-between border-t border-gray-200 dark:border-gray-700">
        <div class="text-sm text-gray-700 dark:text-gray-100">
          الصفحة {page + 1} من {Math.ceil(totalCount / pageSize) || 1}
          <span class="mx-2">|</span>
          إجمالي: {totalCount} نتيجة
        </div>
        <div class="flex gap-2">
          <button
            on:click={prevPage}
            disabled={page === 0 || loading}
            class="px-3 py-1 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded-lg text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-50 dark:bg-gray-900"
          >
            السابق
          </button>
          <button
            on:click={nextPage}
            disabled={!hasMore || loading}
            class="px-3 py-1 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-700 rounded-lg text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-50 dark:bg-gray-900"
          >
            التالي
          </button>
        </div>
      </div>
    </div>
  </div>
</Layout>

<!-- Detail Modal -->
{#if showModal && selectedEntry}
  <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50" dir="rtl">
    <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto">
      <div class="p-6">
        <div class="flex justify-between items-center mb-4">
          <h2 class="text-xl font-bold text-gray-800 dark:text-gray-100">تفاصيل العملية</h2>
          <button
            on:click={closeModal}
            class="text-gray-400 hover:text-gray-600 dark:text-gray-400"
            aria-label="إغلاق"
          >
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
            </svg>
          </button>
        </div>

        <div class="space-y-4">
          <div class="grid grid-cols-2 gap-4">
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">المعرف:</span>
              <span class="text-gray-900 dark:text-gray-100 font-mono text-sm block">{selectedEntry.id}</span>
            </div>
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">الوقت:</span>
              <span class="text-gray-900 dark:text-gray-100 block">{formatTimestamp(selectedEntry.timestamp)}</span>
            </div>
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">المستخدم:</span>
              <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.username}</span>
            </div>
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">الحالة:</span>
              <span class="{selectedEntry.status === 'Success' ? 'text-green-600' : 'text-red-600'} font-medium block">
                {selectedEntry.status === 'Success' ? 'ناجح' : 'فاشل'}
              </span>
            </div>
          </div>

          <div>
            <span class="text-gray-500 dark:text-gray-400 text-sm">العملية:</span>
            <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.action_display}</span>
          </div>

          <div>
            <span class="text-gray-500 dark:text-gray-400 text-sm">نوع الكيان:</span>
            <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.entity_type_display}</span>
          </div>

          {#if selectedEntry.entity_name}
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">اسم الكيان:</span>
              <span class="text-gray-900 dark:text-gray-100 block">{selectedEntry.entity_name}</span>
            </div>
          {/if}

          {#if selectedEntry.old_value}
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">القيمة القديمة:</span>
              <pre class="mt-1 bg-gray-50 dark:bg-gray-900 p-3 rounded-lg text-xs overflow-auto">{JSON.stringify(selectedEntry.old_value, null, 2)}</pre>
            </div>
          {/if}

          {#if selectedEntry.new_value}
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">القيمة الجديدة:</span>
              <pre class="mt-1 bg-gray-50 dark:bg-gray-900 p-3 rounded-lg text-xs overflow-auto">{JSON.stringify(selectedEntry.new_value, null, 2)}</pre>
            </div>
          {/if}

          {#if selectedEntry.error_message}
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">رسالة الخطأ:</span>
              <div class="mt-1 bg-red-50 text-red-700 p-3 rounded-lg text-sm">{selectedEntry.error_message}</div>
            </div>
          {/if}

          {#if selectedEntry.metadata}
            <div>
              <span class="text-gray-500 dark:text-gray-400 text-sm">البيانات الإضافية:</span>
              <pre class="mt-1 bg-gray-50 dark:bg-gray-900 p-3 rounded-lg text-xs overflow-auto">{JSON.stringify(selectedEntry.metadata, null, 2)}</pre>
            </div>
          {/if}
        </div>

        <div class="mt-6 flex justify-end">
          <button
            on:click={closeModal}
            class="px-4 py-2 bg-gray-200 text-gray-700 dark:text-gray-100 rounded-lg hover:bg-gray-300 transition-colors"
          >
            إغلاق
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
