<script lang="ts">
  import { onMount } from 'svelte';
  import { getSystemMetrics, getLoginMetrics, getSettings, syncPreflightCheck } from '../lib/tauri';
  import type { SystemMetrics, LoginMetrics, Settings, User, SyncPreflightCheck } from '../lib/types';
  import { currentUser as userStore } from '../lib/session';
  import Layout from '../components/Layout.svelte';

  let systemMetrics: SystemMetrics | null = null;
  let loginMetrics: LoginMetrics | null = null;
  let settings: Settings | null = null;
  $: currentUser = $userStore;
  let loading = true;
  let error: string | null = null;
  let currentTime = new Date();
  let preflight: SyncPreflightCheck | null = null;

  const timeInterval = setInterval(() => {
    currentTime = new Date();
  }, 1000);

  onMount(() => {
    (async () => {
      await loadSettings();
      loadMetrics();
    })();
    
    const metricsInterval = setInterval(loadMetrics, 30000);
    
    return () => {
      clearInterval(metricsInterval);
      clearInterval(timeInterval);
    };
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
    } catch (e) {
      // Failed to load settings
    }
  }



  async function loadMetrics() {
    try {
      loading = true;
      error = null;
      const username = currentUser?.username;
      
      [systemMetrics, loginMetrics, preflight] = await Promise.all([
        getSystemMetrics(),
        getLoginMetrics(),
        syncPreflightCheck()
      ]);
    } catch (err) {
      error = 'فشل تحميل الإحصائيات';
    } finally {
      loading = false;
    }
  }

  function formatUptime(seconds: number): string {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    
    if (days > 0) {
      return `${days} يوم ${hours} ساعة`;
    } else if (hours > 0) {
      return `${hours} ساعة ${minutes} دقيقة`;
    } else {
      return `${minutes} دقيقة`;
    }
  }

  function formatFileSize(bytes: number): string {
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;
    
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    
    return `${size.toFixed(2)} ${units[unitIndex]}`;
  }

  function getSecurityStatus(metrics: LoginMetrics): { status: string; color: string } {
    if (metrics.current_lockout) {
      return { status: 'محظور', color: 'red' };
    } else if (metrics.failed_attempts > metrics.total_attempts * 0.5) {
      return { status: 'تحذير', color: 'yellow' };
    } else {
      return { status: 'آمن', color: 'green' };
    }
  }

  $: securityStatus = loginMetrics ? getSecurityStatus(loginMetrics) : null;
</script>

<Layout nodeType="WILAYA" title="الإحصائيات">
  <div class="container mx-auto p-6">
  <div class="flex justify-between items-center mb-6">
    <h1 class="text-3xl font-bold text-gray-800 dark:text-white">الإحصائيات</h1>
    <div class="text-sm text-gray-600 dark:text-gray-400">
      آخر تحديث: {currentTime.toLocaleTimeString('ar-DZ')}
    </div>
  </div>

  {#if loading && !systemMetrics}
    <div class="text-center py-12">
      <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
      <p class="text-gray-600 dark:text-gray-400">جاري تحميل الإحصائيات...</p>
    </div>
  {:else if error}
    <div class="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded mb-4">
      {error}
      <button on:click={loadMetrics} class="mt-2 bg-red-600 hover:bg-red-700 text-white px-3 py-1 rounded text-sm">
        إعادة المحاولة
      </button>
    </div>
  {:else}
    <!-- System Overview Cards -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
      <!-- WILAYA specific cards -->
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm font-medium text-gray-600 dark:text-gray-400">المنتجات</p>
            <p class="text-2xl font-bold text-gray-900 dark:text-white">{systemMetrics?.total_products || 0}</p>
          </div>
          <div class="bg-blue-100 dark:bg-blue-900 p-3 rounded-full">
            <svg class="w-6 h-6 text-blue-600 dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"></path>
            </svg>
          </div>
        </div>
      </div>
      
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm font-medium text-gray-600 dark:text-gray-400">الوحدات</p>
            <p class="text-2xl font-bold text-gray-900 dark:text-white">{systemMetrics?.units_count || 0}</p>
          </div>
          <div class="bg-green-100 dark:bg-green-900 p-3 rounded-full">
            <svg class="w-6 h-6 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"></path>
            </svg>
          </div>
        </div>
      </div>

      <!-- Common cards -->
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm font-medium text-gray-600 dark:text-gray-400">حجم قاعدة البيانات</p>
            <p class="text-2xl font-bold text-gray-900 dark:text-white">{systemMetrics ? formatFileSize(systemMetrics.database_size) : '0 KB'}</p>
          </div>
          <div class="bg-gray-100 dark:bg-gray-900 p-3 rounded-full">
            <svg class="w-6 h-6 text-gray-600 dark:text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4"></path>
            </svg>
          </div>
        </div>
      </div>

      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm font-medium text-gray-600 dark:text-gray-400">التقارير الشهرية</p>
            <p class="text-2xl font-bold text-gray-900 dark:text-white">{systemMetrics?.monthly_reports || 0}</p>
          </div>
          <div class="bg-purple-100 dark:bg-purple-900 p-3 rounded-full">
            <svg class="w-6 h-6 text-purple-600 dark:text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v1a1 1 0 001 1h4a1 1 0 001-1v-1m3-2V8a2 2 0 00-2-2H8a2 2 0 00-2 2v6m3 0h6"></path>
            </svg>
          </div>
        </div>
      </div>
    </div>

    <!-- Security Status -->
    {#if loginMetrics}
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8">
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
          <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">حالة الأمان</h2>
          <div class="space-y-4">
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">الحالة الحالية</span>
              {#if securityStatus}
                <span class="px-3 py-1 rounded-full text-sm font-medium
                  {securityStatus.color === 'green' ? 'bg-green-100 text-green-800' : ''}
                  {securityStatus.color === 'yellow' ? 'bg-yellow-100 text-yellow-800' : ''}
                  {securityStatus.color === 'red' ? 'bg-red-100 text-red-800' : ''}">
                  {securityStatus.status}
                </span>
              {/if}
            </div>
            
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">معدل النجاح</span>
              <span class="font-medium text-gray-900 dark:text-white">
                {loginMetrics.total_attempts > 0 
                  ? (((loginMetrics.total_attempts - loginMetrics.failed_attempts) / loginMetrics.total_attempts) * 100).toFixed(1)
                  : '0.0'}%
              </span>
            </div>
            
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">المحاولات الكلية</span>
              <span class="font-medium text-gray-900 dark:text-white">{loginMetrics.total_attempts}</span>
            </div>
            
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">المحاولات الفاشلة</span>
              <span class="font-medium text-red-600">{loginMetrics.failed_attempts}</span>
            </div>
            
            {#if loginMetrics.current_lockout}
              <div class="bg-red-50 dark:bg-red-900/20 p-3 rounded">
                <p class="text-sm text-red-800 dark:text-red-200">
                  الحظر سينتهي خلال {loginMetrics.lockout_time_remaining || 0} دقيقة
                </p>
              </div>
            {/if}
          </div>
        </div>

        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
          <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">نظام التشغيل</h2>
          <div class="space-y-4">
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">وقت التشغيل</span>
              <span class="font-medium text-gray-900 dark:text-white">
                {formatUptime(systemMetrics?.uptime || 0)}
              </span>
            </div>
            
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">معدل ضربات الكاش</span>
              <span class="font-medium text-gray-900 dark:text-white">
                {(systemMetrics?.cache_hit_rate || 0) < 0 
                  ? 'غير متاح' 
                  : ((systemMetrics?.cache_hit_rate || 0) * 100).toFixed(1) + '%'}
              </span>
            </div>
            
            <div class="flex justify-between items-center">
              <span class="text-gray-600 dark:text-gray-400">آخر نسخة احتياطية</span>
              <span class="font-medium text-gray-900 dark:text-white">
                {systemMetrics?.last_backup ? 
                  new Date(systemMetrics.last_backup).toLocaleDateString('ar-DZ') : 
                  'لا يوجد'}
              </span>
            </div>
          </div>
        </div>
      </div>
    {/if}

    {#if preflight}
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-8">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-lg font-semibold text-gray-900 dark:text-white">جاهزية أمان المزامنة</h2>
          <span
            class="px-3 py-1 rounded-full text-sm font-medium
            {preflight.status === 'ok' ? 'bg-green-100 text-green-800' : ''}
            {preflight.status === 'warn' ? 'bg-yellow-100 text-yellow-800' : ''}
            {preflight.status === 'fail' ? 'bg-red-100 text-red-800' : ''}"
          >
            {preflight.status === 'ok' ? 'جاهز' : preflight.status === 'warn' ? 'تحذير' : 'فشل'}
          </span>
        </div>

        <div class="space-y-2">
          {#each preflight.reason_messages_ar as message}
            <div class="text-sm text-gray-700 dark:text-gray-200">- {message}</div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>
</Layout>

<style>
  .container {
    max-width: 1200px;
  }
</style>
