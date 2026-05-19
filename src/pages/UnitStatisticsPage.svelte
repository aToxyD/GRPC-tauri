<script lang="ts">
  import { onMount } from 'svelte';
  import { getSystemMetrics, getLoginMetrics, getSettings } from '../lib/tauri';
  import type { SystemMetrics, LoginMetrics, Settings, User } from '../lib/types';
  import { currentUser as userStore } from '../lib/session';
  import Layout from '../components/Layout.svelte';

  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import { createOperation } from '../lib/operationGuard';
  import { onDestroy } from 'svelte';

  const statsOp = createOperation();
  const loading = statsOp.loading;
  const error = statsOp.error;

  let systemMetrics: SystemMetrics | null = null;
  let loginMetrics: LoginMetrics | null = null;
  let settings: Settings | null = null;
  $: currentUser = $userStore;
  let currentTime = new Date();

  let timeInterval: number | null = null;
  let metricsInterval: number | null = null;

  onMount(() => {
    (async () => {
      await loadSettings();
      await loadMetrics();
    })();
    
    metricsInterval = window.setInterval(loadMetrics, 30000);
    timeInterval = window.setInterval(() => {
      currentTime = new Date();
    }, 1000);
  });

  onDestroy(() => {
    if (metricsInterval !== null) {
      clearInterval(metricsInterval);
    }
    if (timeInterval !== null) {
      clearInterval(timeInterval);
    }
  });

  async function loadSettings() {
    try {
      settings = await getSettings();
    } catch (e) {
      // Failed to load settings
    }
  }

  async function loadMetrics() {
    await statsOp.run(async () => {
      [systemMetrics, loginMetrics] = await Promise.all([
        getSystemMetrics(),
        getLoginMetrics()
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

  $: securityStatus = loginMetrics ? getSecurityStatus(loginMetrics) : null;
</script>

<Layout nodeType="UNIT" title="الإحصائيات">
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
        <AppButton variant="secondary" size="sm" on:click={loadMetrics}>
          إعادة المحاولة
        </AppButton>
      </AppAlert>
    {:else}
      <!-- System Overview Cards -->
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        <!-- UNIT specific cards -->
        <AppCard>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium text-gray-600 dark:text-gray-400">الطلبات اليوم</p>
              <p class="text-2xl font-bold text-gray-900 dark:text-white mt-1">{systemMetrics?.today_orders || 0}</p>
            </div>
            <div class="bg-orange-50 dark:bg-orange-900/20 p-3 rounded-full">
              <svg class="w-6 h-6 text-orange-600 dark:text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 3h2l.4 2M7 13h10l4-8H5.4M7 13L5.4 5M7 13l-2.293 2.293c-.63.63-.184 1.707.707 1.707H17m0 0a2 2 0 100 4 2 2 0 000-4zm-8 2a2 2 0 11-4 0 2 2 0 014 0z"></path>
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
              <p class="text-sm font-medium text-gray-600 dark:text-gray-400">التقارير اليومية</p>
              <p class="text-2xl font-bold text-gray-900 dark:text-white mt-1">{systemMetrics?.daily_reports || 0}</p>
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
    {/if}
  </div>
</Layout>
