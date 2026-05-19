<script lang="ts">
  import { onMount } from 'svelte';
  import { getAuditHealth, getAuditChainStatus, getSettings } from '../lib/tauri';
  import { createOperation } from '../lib/operationGuard';
  import type { AuditHealthReport, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppSection from '../lib/components/ui/AppSection.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const op = createOperation();
  const loading = op.loading;
  const error = op.error;
  let health: AuditHealthReport | null = null;
  let settings: Settings | null = null;

  async function loadHealth() {
    await op.run(async () => {
      health = await getAuditHealth();
    });
  }

  function fmt(ts: string | null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }
  function truncHash(h: string | null) { return h ? h.slice(0, 20) + '...' : '—'; }

  function sevIntent(s: string): 'danger' | 'warning' | 'info' {
    if (s === 'CRITICAL' || s === 'ERROR') return 'danger';
    if (s === 'WARNING') return 'warning';
    return 'info';
  }

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await loadHealth();
  });

  $: nodeType = (settings?.node_type === 'WILAYA' ? 'WILAYA' : settings?.node_type === 'UNIT' ? 'UNIT' : null) as 'WILAYA' | 'UNIT' | null;
  $: valid = health?.chainStatus?.isValid ?? null;
</script>

<Layout {nodeType} title="سلامة التدقيق" subtitle="مراقبة سلسلة التدقيق">
  <div class="p-6 max-w-5xl mx-auto" dir="rtl">
    
    <AppPageHeader title="🔐 لوحة سلامة التدقيق" subtitle="مراقبة وتحقق من سلسلة Hash التدقيق">
      <svelte:fragment slot="actions">
        <AppButton variant="secondary" size="sm" loading={$loading} on:click={loadHealth}>⟳ تحديث</AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    {#if $loading && !health}
      <AppLoadingState message="جارٍ التحميل..." />
    {:else if health}
      <!-- Chain Banner -->
      <div class="mb-6">
        {#if valid}
          <AppAlert intent="success" title="سلسلة التدقيق سليمة">
            {health.chainStatus.verifiedEntries.toLocaleString()} سجل مُتحقق من {health.chainStatus.totalEntries.toLocaleString()}
          </AppAlert>
        {:else}
          <AppAlert intent="danger" title="تحذير: الإخلال في سلسلة التدقيق!">
            {health.chainStatus.verifiedEntries.toLocaleString()} سجل مُتحقق من {health.chainStatus.totalEntries.toLocaleString()}
          </AppAlert>
        {/if}
      </div>

      <!-- Metrics -->
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <AppCard padding="sm" class="text-center border-t-4 border-blue-500">
          <div class="text-2xl mb-2">📊</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{health.chainStatus.totalEntries.toLocaleString()}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">إجمالي السجلات</div>
        </AppCard>
        <AppCard padding="sm" class="text-center border-t-4 border-green-500">
          <div class="text-2xl mb-2">🔗</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{health.chainStatus.verifiedEntries.toLocaleString()}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">سجلات مُتحققة</div>
        </AppCard>
        <AppCard padding="sm" class="text-center border-t-4 border-purple-500">
          <div class="text-2xl mb-2">🔑</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white font-mono text-sm">{truncHash(health.chainStatus.latestHash)}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">آخر Hash</div>
        </AppCard>
        <AppCard padding="sm" class="text-center border-t-4 border-amber-500">
          <div class="text-2xl mb-2">⚠️</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{health.anomalies.length}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">شذوذات</div>
        </AppCard>
      </div>

      <!-- Info -->
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
        <AppCard padding="md" class="border-t-4 border-gray-200 dark:border-gray-700">
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">📅 آخر عمليات التزامن</h2>
          <div class="flex flex-col gap-2">
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر استيراد</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastImportTimestamp)}</span></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر تصدير</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastExportTimestamp)}</span></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر نسخة احتياطية</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastBackupTimestamp)}</span></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر استعادة</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastRestoreTimestamp)}</span></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر إدخال</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.chainStatus.latestEntryTimestamp)}</span></div>
          </div>
        </AppCard>
        
        <AppCard padding="md" class="border-t-4 border-gray-200 dark:border-gray-700">
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🔒 Hash الأخير</h2>
          {#if health.chainStatus.latestHash}
            <div class="bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg p-3 break-all">
              <code class="font-mono text-xs text-gray-800 dark:text-gray-300">{health.chainStatus.latestHash}</code>
            </div>
          {:else}
            <p class="text-gray-400 dark:text-gray-500 italic">لا توجد سجلات مُجزأة</p>
          {/if}
          {#if !valid && health.chainStatus.breakDescription}
            <div class="mt-3">
              <AppAlert intent="danger">{health.chainStatus.breakDescription}</AppAlert>
            </div>
          {/if}
        </AppCard>
      </div>

      <!-- Anomalies -->
      <AppSection title="⚠️ الشذوذات ({health.anomalies.length})">
        {#if health.anomalies.length > 0}
          <div class="flex flex-col gap-2">
            {#each health.anomalies as a}
              <AppAlert intent={sevIntent(a.severity)}>
                <div class="flex gap-3 items-center mb-1.5 flex-wrap">
                  <AppBadge intent={sevIntent(a.severity)}>{a.severity}</AppBadge>
                  <code>{a.anomalyType}</code>
                  <span class="text-xs opacity-70 mr-auto">{fmt(a.detectedAt)}</span>
                </div>
                <p class="text-sm">{a.description}</p>
              </AppAlert>
            {/each}
          </div>
        {:else}
          <AppEmptyState
            title="لا توجد شذوذات"
            description="النظام يعمل بشكل طبيعي"
            icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        {/if}
      </AppSection>
    {/if}
  </div>
</Layout>
