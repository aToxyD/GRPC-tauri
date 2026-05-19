<script lang="ts">
  import { onMount } from 'svelte';
  import { getSystemHealth, getSyncHealth, getSettings, getBuildInfo, getRecentTelemetry } from '../lib/tauri';
  import type { SystemHealthReport, SyncNodeHealth, Settings, BuildInfo, TelemetryEvent } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';

  let health: SystemHealthReport | null = null;
  let nodes: SyncNodeHealth[] = [];
  let buildInfo: BuildInfo | null = null;
  let telemetry: TelemetryEvent[] = [];
  let loading = true;
  let error: string | null = null;
  let settings: Settings | null = null;

  async function load() {
    loading = true; error = null;
    try {
      [health, nodes, buildInfo, telemetry] = await Promise.all([
        getSystemHealth(), 
        getSyncHealth(), 
        getBuildInfo(),
        getRecentTelemetry(20)
      ]);
    } catch(e) { error = e instanceof Error ? e.message : 'فشل التحميل'; }
    finally { loading = false; }
  }

  function fmtBytes(b: number): string {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b/1024).toFixed(1) + ' KB';
    return (b/1048576).toFixed(1) + ' MB';
  }
  function fmt(ts: string|null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }
  
  function statusIcon(s: string) {
    return s==='HEALTHY'?'✅':s==='DEGRADED'?'⚠️':s==='CRITICAL'?'🔴':'❓';
  }
  
  function statusIntent(s: string): 'success' | 'warning' | 'danger' | 'neutral' {
    return s === 'HEALTHY' ? 'success' : s === 'DEGRADED' ? 'warning' : s === 'CRITICAL' ? 'danger' : 'neutral';
  }

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await load();
  });

  $: nodeType = (settings?.node_type === 'WILAYA' ? 'WILAYA' : settings?.node_type === 'UNIT' ? 'UNIT' : null) as 'WILAYA' | 'UNIT' | null;
</script>

<Layout {nodeType} title="صحة النظام" subtitle="لوحة المراقبة التشغيلية الشاملة">
  <div dir="rtl">
    <AppPageHeader title="صحة النظام" subtitle="المراقبة التشغيلية الشاملة للعقدة">
      <svelte:fragment slot="actions">
        <AppButton variant="primary" on:click={load} disabled={loading}>⟳ تحديث</AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if error}
      <div class="mb-4">
        <AppAlert intent="danger">{error}</AppAlert>
      </div>
    {/if}

    {#if loading && !health}
      <AppLoadingState message="جارٍ تحميل بيانات صحة النظام..." />
    {:else if health}
      <!-- Status Cards -->
      <div class="mb-6">
        <AppAlert intent="info" title="دليل حالات النظام (Integrity States):">
          <ul class="list-disc pr-5 space-y-1 mt-2 text-sm">
            <li><strong>✅ HEALTHY:</strong> النظام سليم ولا توجد مشاكل.</li>
            <li><strong>⚠️ DEGRADED / WARNING:</strong> هناك تعارضات أو مشاكل طفيفة يجب مراجعتها، لكن العمليات الأساسية مستمرة.</li>
            <li><strong>🔴 CRITICAL / CORRUPTED:</strong> تم اكتشاف تلف أو تلاعب. النظام في حالة حماية (Fail-Closed). يُمنع إدخال بيانات جديدة ويجب الاستعانة بنسخة احتياطية.</li>
          </ul>
        </AppAlert>
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <AppCard class="border-2 text-center hover:-translate-y-0.5 transition-transform {health.databaseStatus.status === 'HEALTHY' ? 'border-green-300 dark:border-green-800' : health.databaseStatus.status === 'DEGRADED' ? 'border-yellow-300 dark:border-yellow-800' : 'border-red-300 dark:border-red-800'}" padding="sm">
          <div class="text-3xl mb-1.5">{statusIcon(health.databaseStatus.status)}</div>
          <div class="font-bold text-base dark:text-white">قاعدة البيانات</div>
          <div class="text-xs mt-1 dark:text-gray-300">{health.databaseStatus.message}</div>
        </AppCard>
        
        <AppCard class="border-2 text-center hover:-translate-y-0.5 transition-transform {health.backupStatus.status === 'HEALTHY' ? 'border-green-300 dark:border-green-800' : health.backupStatus.status === 'DEGRADED' ? 'border-yellow-300 dark:border-yellow-800' : 'border-red-300 dark:border-red-800'}" padding="sm">
          <div class="text-3xl mb-1.5">{statusIcon(health.backupStatus.status)}</div>
          <div class="font-bold text-base dark:text-white">النسخ الاحتياطية</div>
          <div class="text-xs mt-1 dark:text-gray-300">{health.backupStatus.message}</div>
        </AppCard>
        
        <AppCard class="border-2 text-center hover:-translate-y-0.5 transition-transform {health.auditStatus.status === 'HEALTHY' ? 'border-green-300 dark:border-green-800' : health.auditStatus.status === 'DEGRADED' ? 'border-yellow-300 dark:border-yellow-800' : 'border-red-300 dark:border-red-800'}" padding="sm">
          <div class="text-3xl mb-1.5">{statusIcon(health.auditStatus.status)}</div>
          <div class="font-bold text-base dark:text-white">سلسلة التدقيق</div>
          <div class="text-xs mt-1 dark:text-gray-300">{health.auditStatus.message}</div>
        </AppCard>
        
        <AppCard class="border-2 text-center hover:-translate-y-0.5 transition-transform {health.syncStatus.status === 'HEALTHY' ? 'border-green-300 dark:border-green-800' : health.syncStatus.status === 'DEGRADED' ? 'border-yellow-300 dark:border-yellow-800' : 'border-red-300 dark:border-red-800'}" padding="sm">
          <div class="text-3xl mb-1.5">{statusIcon(health.syncStatus.status)}</div>
          <div class="font-bold text-base dark:text-white">المزامنة</div>
          <div class="text-xs mt-1 dark:text-gray-300">{health.syncStatus.message}</div>
        </AppCard>
      </div>

      <!-- Metrics -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
        <!-- Backup Details -->
        <AppCard>
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">💾 تفاصيل النسخ الاحتياطية</h2>
          <div class="flex flex-col gap-2">
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">عدد النسخ</span><strong class="text-gray-800 dark:text-white">{health.backupStatus.backupCount}</strong></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر نسخة</span><strong class="text-gray-800 dark:text-white">{fmt(health.backupStatus.lastBackup)}</strong></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">عمر آخر نسخة</span>
              <strong class="text-gray-800 dark:text-white">{health.backupStatus.backupAgeDays !== null ? health.backupStatus.backupAgeDays + ' يوم' : '—'}</strong>
            </div>
          </div>
        </AppCard>

        <!-- Sync Details -->
        <AppCard>
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🔄 تفاصيل المزامنة</h2>
          <div class="flex flex-col gap-2">
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">استيرادات فاشلة</span><strong class="text-gray-800 dark:text-white {health.syncStatus.failedImportsCount>0?'text-red-600 dark:text-red-400':''}">{health.syncStatus.failedImportsCount}</strong></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">تعارضات غير محلولة</span><strong class="text-gray-800 dark:text-white {health.syncStatus.unresolvedConflicts>0?'text-red-600 dark:text-red-400':''}">{health.syncStatus.unresolvedConflicts}</strong></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">حزم مكررة</span><strong class="text-gray-800 dark:text-white">{health.syncStatus.duplicatePackageAttempts}</strong></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر مزامنة ناجحة</span><strong class="text-gray-800 dark:text-white">{fmt(health.syncStatus.lastSuccessfulSync)}</strong></div>
          </div>
        </AppCard>

        <!-- Storage -->
        <AppCard>
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🗄️ التخزين والذاكرة</h2>
          <div class="flex flex-col gap-2">
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">حجم قاعدة البيانات</span><strong class="text-gray-800 dark:text-white">{fmtBytes(health.storageUsageBytes)}</strong></div>
            <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">تاريخ التقرير</span><strong class="text-gray-800 dark:text-white">{fmt(health.generatedAt)}</strong></div>
          </div>
          <!-- Storage bar -->
          <div class="h-2 bg-gray-200 dark:bg-gray-700 rounded-full mt-4 overflow-hidden">
            <div class="h-full bg-gradient-to-r from-blue-500 to-purple-500 rounded-full transition-all duration-500" style="width:{Math.min((health.storageUsageBytes/10485760)*100,100)}%"></div>
          </div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1 text-center">{fmtBytes(health.storageUsageBytes)} / 10 MB (تقدير)</div>
        </AppCard>
      </div>

      <!-- Node Table -->
      {#if nodes.length > 0}
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🌐 صحة العقد المزامنة ({nodes.length})</h2>
        <AppCard padding="none" class="mb-6">
          <AppTable>
            <svelte:fragment slot="head">
              <th class="table-header text-right">الحالة</th>
              <th class="table-header text-right">معرف العقدة</th>
              <th class="table-header text-right">آخر مزامنة</th>
              <th class="table-header text-right">حزم مستلمة</th>
              <th class="table-header text-right">حزم مرفوضة</th>
              <th class="table-header text-right">إعادة تشغيل</th>
            </svelte:fragment>
            
            {#each nodes as n}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="table-cell">
                  <AppBadge intent={statusIntent(n.status)} size="sm">{statusIcon(n.status)} {n.status}</AppBadge>
                </td>
                <td class="table-cell"><code class="dark:text-gray-300">{n.nodeId}</code></td>
                <td class="table-cell dark:text-gray-300">{fmt(n.lastSyncTimestamp)}</td>
                <td class="table-cell dark:text-gray-300">{n.packagesReceived}</td>
                <td class="table-cell {n.packagesRejected>0?'text-red-600 dark:text-red-400':'dark:text-gray-300'}">{n.packagesRejected}</td>
                <td class="table-cell {n.replayAttempts>0?'text-orange-600 dark:text-orange-400':'dark:text-gray-300'}">{n.replayAttempts}</td>
              </tr>
            {/each}
          </AppTable>
        </AppCard>
      {:else}
        <div class="text-center p-8 bg-gray-50 dark:bg-gray-900/50 rounded-xl text-gray-400 mb-6 border border-gray-200 dark:border-gray-800">لا توجد بيانات لعقد المزامنة بعد</div>
      {/if}

      <!-- Operational Telemetry -->
      {#if telemetry.length > 0}
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3 mt-6">📊 السجلات التشغيلية (Operational Telemetry)</h2>
        <AppCard padding="none" class="mb-6">
          <AppTable>
            <svelte:fragment slot="head">
              <th class="table-header text-right">التوقيت</th>
              <th class="table-header text-right">العملية</th>
              <th class="table-header text-right">النتيجة</th>
              <th class="table-header text-right">المدة</th>
              <th class="table-header text-right">المستخدم</th>
            </svelte:fragment>

            {#each telemetry as t}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="table-cell text-xs dark:text-gray-400">{fmt(t.timestamp)}</td>
                <td class="table-cell"><code class="text-xs dark:text-gray-300">{t.event_type}</code></td>
                <td class="table-cell">
                  <AppBadge intent={t.outcome === 'SUCCESS' ? 'success' : 'danger'} size="sm">{t.outcome}</AppBadge>
                </td>
                <td class="table-cell dark:text-gray-300">{t.duration_ms !== null ? t.duration_ms + ' ms' : '—'}</td>
                <td class="table-cell text-xs dark:text-gray-400">{t.user_id || 'System'}</td>
              </tr>
            {/each}
          </AppTable>
        </AppCard>
      {/if}

      <!-- Build Metadata -->
      {#if buildInfo}
        <AppCard class="mt-6 bg-slate-50 dark:bg-gray-900/50">
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🛠️ معلومات الإصدار والبناء (Build Information)</h2>
          <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">إصدار التطبيق</span><code class="dark:text-gray-300 bg-white dark:bg-gray-800 p-1 rounded inline-block w-max">{buildInfo.app_version}</code></div>
            <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">معرف الالتزام (Git)</span><code class="dark:text-gray-300 bg-white dark:bg-gray-800 p-1 rounded inline-block w-max">{buildInfo.git_commit}</code></div>
            <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">إصدار المخطط</span><code class="dark:text-gray-300 bg-white dark:bg-gray-800 p-1 rounded inline-block w-max">v{buildInfo.schema_version}</code></div>
            <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">تاريخ البناء</span><span class="text-xs text-slate-500 dark:text-slate-400">{fmt(new Date(parseInt(buildInfo.build_timestamp)*1000).toISOString())}</span></div>
          </div>
        </AppCard>
      {/if}
    {/if}
  </div>
</Layout>
