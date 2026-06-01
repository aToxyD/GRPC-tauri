<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getSystemMetrics, getLoginMetrics, syncPreflightCheck } from '../lib/contracts';
  import { getSettings } from '../lib/contracts';
  import type { SystemMetrics, LoginMetrics, Settings, SyncPreflightCheck } from '../lib/types';
  import { currentUser as userStore } from '../lib/session';
  import Layout from '../components/Layout.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { createRuntimeScope } from '../lib/runtimeCleanup';

  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';

  const scope = createRuntimeScope();
  const metricsOp = createOperation({ scope });
  const loading = metricsOp.loading;
  const error = metricsOp.error;

  // @category ProjectionState
  let systemMetrics: SystemMetrics | null = null;
  // @category ProjectionState
  let loginMetrics: LoginMetrics | null = null;
  // @category ProjectionState
  let settings: Settings | null = null;
  // @category SessionState
  $: currentUser = $userStore;
  // @category UiState
  let currentTime = new Date();
  // @category ProjectionState
  let preflight: SyncPreflightCheck | null = null;

  onMount(async () => {
    scope.setInterval(() => {
      currentTime = new Date();
    }, 1000);

    await loadSettings();
    await loadMetrics();

    scope.setInterval(loadMetrics, 30000);
  });

  onDestroy(() => scope.dispose());

  async function loadSettings() {
    try {
      settings = await getSettings();
    } catch (e) {
      // Failed to load settings
    }
  }

  async function loadMetrics() {
    await metricsOp.run(async () => {
      [systemMetrics, loginMetrics, preflight] = await Promise.all([
        getSystemMetrics(),
        getLoginMetrics(),
        syncPreflightCheck()
      ]);
    });
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

  function getSecurityStatus(metrics: LoginMetrics): { status: string; intent: 'success' | 'warning' | 'danger' } {
    if (metrics.current_lockout) {
      return { status: 'محظور', intent: 'danger' };
    } else if (metrics.failed_attempts > metrics.total_attempts * 0.5) {
      return { status: 'تحذير', intent: 'warning' };
    } else {
      return { status: 'آمن', intent: 'success' };
    }
  }

  // @category UiState
  $: securityStatus = loginMetrics ? getSecurityStatus(loginMetrics) : null;
</script>

<Layout nodeType="WILAYA" title="الإحصائيات">
  <div class="max-w-6xl mx-auto p-6" dir="rtl">
    <div class="flex justify-between items-center mb-6">
      <h1 class="text-3xl font-bold text-gray-800 dark:text-white">الإحصائيات</h1>
      <div class="text-sm text-gray-600 dark:text-gray-400 bg-white dark:bg-gray-800 px-3 py-1.5 rounded-full shadow-sm border border-gray-100 dark:border-gray-700">
        آخر تحديث: {currentTime.toLocaleTimeString('ar-DZ')}
      </div>
    </div>

    {#if $loading && !systemMetrics}
      <AppLoadingState message="جاري تحميل الإحصائيات..." />
    {:else if $error}
      <AppAlert intent="danger" title="خطأ">
        <p class="mb-2">{$error}</p>
        <AppButton variant="secondary" size="sm" on:click={loadMetrics} disabled={$loading}>
          إعادة المحاولة
        </AppButton>
      </AppAlert>
    {:else}
      <!-- System Overview Cards -->
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        <!-- WILAYA specific cards -->
        <AppCard>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium text-gray-600 dark:text-gray-400">المنتجات</p>
              <p class="text-2xl font-bold text-gray-900 dark:text-white mt-1">{systemMetrics?.total_products || 0}</p>
            </div>
            <div class="bg-blue-50 dark:bg-blue-900/20 p-3 rounded-full">
              <svg class="w-6 h-6 text-civil-blue dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"></path>
              </svg>
            </div>
          </div>
        </AppCard>
        
        <AppCard>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium text-gray-600 dark:text-gray-400">الوحدات</p>
              <p class="text-2xl font-bold text-gray-900 dark:text-white mt-1">{systemMetrics?.units_count || 0}</p>
            </div>
            <div class="bg-green-50 dark:bg-green-900/20 p-3 rounded-full">
              <svg class="w-6 h-6 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"></path>
              </svg>
            </div>
          </div>
        </AppCard>

        <!-- Common cards -->
        <AppCard>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium text-gray-600 dark:text-gray-400">حجم قاعدة البيانات</p>
              <p class="text-2xl font-bold text-gray-900 dark:text-white mt-1">{systemMetrics ? formatFileSize(systemMetrics.database_size) : '0 KB'}</p>
            </div>
            <div class="bg-gray-100 dark:bg-gray-800 p-3 rounded-full">
              <svg class="w-6 h-6 text-gray-600 dark:text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4"></path>
              </svg>
            </div>
          </div>
        </AppCard>

        <AppCard>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium text-gray-600 dark:text-gray-400">التقارير الشهرية</p>
              <p class="text-2xl font-bold text-gray-900 dark:text-white mt-1">{systemMetrics?.monthly_reports || 0}</p>
            </div>
            <div class="bg-purple-50 dark:bg-purple-900/20 p-3 rounded-full">
              <svg class="w-6 h-6 text-purple-600 dark:text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v1a1 1 0 001 1h4a1 1 0 001-1v-1m3-2V8a2 2 0 00-2-2H8a2 2 0 00-2 2v6m3 0h6"></path>
              </svg>
            </div>
          </div>
        </AppCard>
      </div>

      <!-- Security Status -->
      {#if loginMetrics}
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8">
          <AppCard>
            <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4 pb-2 border-b border-gray-100 dark:border-gray-700">حالة الأمان</h2>
            <div class="space-y-4">
              <div class="flex justify-between items-center">
                <span class="text-gray-600 dark:text-gray-400">الحالة الحالية</span>
                {#if securityStatus}
                  <AppBadge intent={securityStatus.intent} size="sm">
                    {securityStatus.status}
                  </AppBadge>
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
                <span class="font-medium text-red-600 dark:text-red-400">{loginMetrics.failed_attempts}</span>
              </div>
              
              {#if loginMetrics.current_lockout}
                <div class="bg-red-50 dark:bg-red-900/20 p-3 rounded-lg border border-red-100 dark:border-red-800">
                  <p class="text-sm text-red-800 dark:text-red-200">
                    الحظر سينتهي خلال {loginMetrics.lockout_time_remaining || 0} دقيقة
                  </p>
                </div>
              {/if}
            </div>
          </AppCard>

          <AppCard>
            <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4 pb-2 border-b border-gray-100 dark:border-gray-700">نظام التشغيل</h2>
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
          </AppCard>
        </div>
      {/if}

      {#if preflight}
        <AppCard class="mb-8">
          <div class="flex items-center justify-between mb-4 pb-2 border-b border-gray-100 dark:border-gray-700">
            <h2 class="text-lg font-semibold text-gray-900 dark:text-white">جاهزية أمان المزامنة</h2>
            <AppBadge intent={preflight.status === 'ok' ? 'success' : preflight.status === 'warn' ? 'warning' : 'danger'}>
              {preflight.status === 'ok' ? 'جاهز' : preflight.status === 'warn' ? 'تحذير' : 'فشل'}
            </AppBadge>
          </div>

          <ul class="space-y-2">
            {#each preflight.reason_messages_ar as message}
              <li class="text-sm text-gray-700 dark:text-gray-300 flex items-center">
                <span class="w-1.5 h-1.5 bg-gray-400 rounded-full mr-2 ml-2"></span>
                {message}
              </li>
            {/each}
          </ul>
        </AppCard>
      {/if}
    {/if}
  </div>
</Layout>
