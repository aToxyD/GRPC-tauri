<script lang="ts">
  import Router from 'svelte-spa-router';
  import { link } from 'svelte-spa-router';
  import { onMount, onDestroy } from 'svelte';
  import { createRuntimeScope } from './lib/runtimeCleanup';
  import { bootstrapSession, cleanupSessionManagement } from './lib/session';
  import { hasFatalError, fatalErrorMessage } from './lib/errorBoundary';
  import { telemetry } from './lib/telemetry';
  
  // Import page components
  import LoginPage from './pages/LoginPage.svelte';
  import WilayaNodeSetupPage from './pages/WilayaNodeSetupPage.svelte';
  import WilayaDashboard from './pages/WilayaDashboard.svelte';
  import ProductsPage from './pages/ProductsPage.svelte';
  import UnitsPage from './pages/UnitsPage.svelte';
  import SyncPage from './pages/SyncPage.svelte';
  import UnitDashboard from './pages/UnitDashboard.svelte';
  import OrdersPage from './pages/OrdersPage.svelte';
  import ConsumptionPage from './pages/ConsumptionPage.svelte';
  import WilayaReportsPage from './pages/WilayaReportsPage.svelte';
  import UnitReportsPage from './pages/UnitReportsPage.svelte';
  import StockPage from './pages/StockPage.svelte';
  import BackupPage from './pages/BackupPage.svelte';
  import WilayaStatisticsPage from './pages/WilayaStatisticsPage.svelte';
  import UnitStatisticsPage from './pages/UnitStatisticsPage.svelte';
  import UnitInventoryPage from './pages/UnitInventoryPage.svelte';
  import AuditLogPage from './pages/AuditLogPage.svelte';
  import AuditIntegrityPage from './pages/AuditIntegrityPage.svelte';
  import SystemHealthPage from './pages/SystemHealthPage.svelte';
  import SyncTopologyPage from './pages/SyncTopologyPage.svelte';
  import ConflictCenterPage from './pages/ConflictCenterPage.svelte';
  import FiscalManagementPage from './pages/FiscalManagementPage.svelte';
  import FiscalDiagnosticsPage from './pages/FiscalDiagnosticsPage.svelte';
  import NotFoundPage from './pages/NotFoundPage.svelte';
  import AppSecurityPage from './pages/AppSecurityPage.svelte';
  
  // Import components
  import Notifications from './components/Notifications.svelte';

  let sessionScope: ReturnType<typeof createRuntimeScope>;
  let mountTime = 0;

  // Initialize session management on mount
  onMount(async () => {
    const startTime = performance.now();
    sessionScope = createRuntimeScope();
    await bootstrapSession(sessionScope);
    mountTime = performance.now() - startTime;
    telemetry.trackPageLoad('App', mountTime);
  });

  // Cleanup session management on destroy
  onDestroy(() => {
    if (sessionScope) sessionScope.dispose();
    cleanupSessionManagement();
  });

  // Define routes
  const routes = {
    '/': LoginPage,
    '/login': LoginPage,
    '/security': AppSecurityPage,
    '/configure': WilayaNodeSetupPage,
    '/wilaya': WilayaDashboard,
    '/wilaya/dashboard': WilayaDashboard,
    '/wilaya/products': ProductsPage,
    '/wilaya/units': UnitsPage,
    '/wilaya/sync': SyncPage,
    '/unit': UnitDashboard,
    '/unit/dashboard': UnitDashboard,
    '/unit/stock': StockPage,
    '/unit/orders': OrdersPage,
    '/unit/consumption': ConsumptionPage,
    '/unit/reports': UnitReportsPage,
    '/wilaya/reports': WilayaReportsPage,
    '/wilaya/statistics': WilayaStatisticsPage,
    '/wilaya/unit-inventory': UnitInventoryPage,
    '/unit/statistics': UnitStatisticsPage,
    '/backup': BackupPage,
    '/audit-log': AuditLogPage,
    '/admin/audit-integrity': AuditIntegrityPage,
    '/admin/system-health': SystemHealthPage,
    '/admin/sync-topology': SyncTopologyPage,
    '/admin/conflicts': ConflictCenterPage,
    '/admin/fiscal': FiscalManagementPage,
    '/admin/diagnostics': FiscalDiagnosticsPage,
    // Fallback to 404
    '*': NotFoundPage
  };

  function handleReload() {
    window.location.reload();
  }

  // Make link function available globally
  export { link };
</script>

<div class="min-h-screen bg-gray-50 dark:bg-gray-900 flex flex-col">
  {#if $hasFatalError}
    <div class="flex-1 flex items-center justify-center p-6 dir-rtl" style="direction: rtl;">
      <div class="max-w-md w-full bg-white dark:bg-gray-800 border border-red-200 dark:border-red-900 rounded-2xl p-8 shadow-2xl text-center space-y-6 transform transition-all duration-300 scale-100">
        <div class="mx-auto w-16 h-16 bg-red-100 dark:bg-red-900/30 rounded-full flex items-center justify-center text-red-600 dark:text-red-400">
          <svg class="w-10 h-10" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"></path>
          </svg>
        </div>
        <div class="space-y-2">
          <h1 class="text-2xl font-bold text-gray-900 dark:text-white">خطأ فادح في النظام</h1>
          <p class="text-gray-600 dark:text-gray-400 leading-relaxed text-sm">
            {$fatalErrorMessage || 'حدث خطأ غير متوقع في النظام. تم تأمين التطبيق لمنع تلف البيانات.'}
          </p>
        </div>
        <button
          on:click={handleReload}
          class="w-full bg-red-600 hover:bg-red-700 active:bg-red-800 text-white font-medium py-3 px-4 rounded-xl transition-colors shadow-lg shadow-red-500/20 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 dark:focus:ring-offset-gray-900"
        >
          إعادة تشغيل التطبيق
        </button>
      </div>
    </div>
  {:else}
    <Router {routes} />
  {/if}
  <Notifications />
</div>
