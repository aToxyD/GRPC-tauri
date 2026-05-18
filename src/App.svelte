<script>
  import Router from 'svelte-spa-router';
  import { link } from 'svelte-spa-router';
  import { onMount, onDestroy } from 'svelte';
  import { bootstrapSession, cleanupSessionManagement } from './lib/session';
  
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
  
  // Import components
  import Notifications from './components/Notifications.svelte';

  // Initialize session management on mount
  onMount(async () => {
    await bootstrapSession();
  });

  // Cleanup session management on destroy
  onDestroy(() => {
    cleanupSessionManagement();
  });

  // Define routes
  const routes = {
    '/': LoginPage,
    '/login': LoginPage,
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

  // Make link function available globally
  export { link };
</script>

<div class="min-h-screen bg-gray-50 dark:bg-gray-900">
  <Router {routes} />
  <Notifications />
</div>
