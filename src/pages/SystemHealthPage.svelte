<script lang="ts">
  import { onMount } from 'svelte';
  import { getSystemHealth, getSyncHealth, getSettings, getBuildInfo, getRecentTelemetry } from '../lib/tauri';
  import type { SystemHealthReport, SyncNodeHealth, Settings, BuildInfo, TelemetryEvent } from '../lib/types';
  import Layout from '../components/Layout.svelte';

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
  function statusClass(s: string) {
    return s==='HEALTHY'?'bg-green-50 dark:bg-green-900/20 border-green-300 dark:border-green-800 text-green-600 dark:text-green-400':s==='DEGRADED'?'bg-yellow-50 dark:bg-yellow-900/20 border-yellow-300 dark:border-yellow-800 text-yellow-600 dark:text-yellow-400':s==='CRITICAL'?'bg-red-50 dark:bg-red-900/20 border-red-300 dark:border-red-800 text-red-600 dark:text-red-400':'bg-slate-50 dark:bg-slate-800 border-slate-200 dark:border-slate-700 text-slate-600 dark:text-slate-400';
  }

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await load();
  });

  $: nodeType = (settings?.node_type === 'WILAYA' ? 'WILAYA' : settings?.node_type === 'UNIT' ? 'UNIT' : null) as 'WILAYA' | 'UNIT' | null;
</script>

<Layout {nodeType} title="صحة النظام" subtitle="لوحة المراقبة التشغيلية الشاملة">
<div class="p-6 max-w-6xl mx-auto" dir="rtl">
  <div class="flex justify-between items-start mb-6">
    <div>
      <h1 class="text-2xl font-bold text-gray-800 dark:text-white">🖥️ صحة النظام</h1>
      <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">المراقبة التشغيلية الشاملة للعقدة</p>
    </div>
    <button class="bg-blue-50 dark:bg-blue-900/200 hover:bg-blue-600 disabled:opacity-60 text-white px-5 py-2 rounded-lg transition-colors cursor-pointer" on:click={load} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 text-red-600 dark:text-red-400 px-4 py-3 rounded-lg mb-4">{error}</div>{/if}

  {#if loading && !health}
    <div class="text-center p-12 bg-white dark:bg-gray-800 rounded-xl shadow">جارٍ تحميل بيانات صحة النظام...</div>
  {:else if health}
    <!-- Status Cards -->
    <div class="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 p-4 rounded-xl mb-6 text-sm text-blue-800 dark:text-blue-300">
      <h3 class="font-bold mb-1">دليل حالات النظام (Integrity States):</h3>
      <ul class="list-disc pr-5 space-y-1">
        <li><strong>✅ HEALTHY:</strong> النظام سليم ولا توجد مشاكل.</li>
        <li><strong>⚠️ DEGRADED / WARNING:</strong> هناك تعارضات أو مشاكل طفيفة يجب مراجعتها، لكن العمليات الأساسية مستمرة.</li>
        <li><strong>🔴 CRITICAL / CORRUPTED:</strong> تم اكتشاف تلف أو تلاعب. النظام في حالة حماية (Fail-Closed). يُمنع إدخال بيانات جديدة ويجب الاستعانة بنسخة احتياطية.</li>
      </ul>
    </div>
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      <div class="rounded-xl p-5 shadow text-center border-2 hover:-translate-y-0.5 transition-transform {statusClass(health.databaseStatus.status)}">
        <div class="text-3xl mb-1.5">{statusIcon(health.databaseStatus.status)}</div>
        <div class="font-bold text-base dark:text-white">قاعدة البيانات</div>
        <div class="text-xs mt-1 dark:text-gray-300">{health.databaseStatus.message}</div>
      </div>
      <div class="rounded-xl p-5 shadow text-center border-2 hover:-translate-y-0.5 transition-transform {statusClass(health.backupStatus.status)}">
        <div class="text-3xl mb-1.5">{statusIcon(health.backupStatus.status)}</div>
        <div class="font-bold text-base dark:text-white">النسخ الاحتياطية</div>
        <div class="text-xs mt-1 dark:text-gray-300">{health.backupStatus.message}</div>
      </div>
      <div class="rounded-xl p-5 shadow text-center border-2 hover:-translate-y-0.5 transition-transform {statusClass(health.auditStatus.status)}">
        <div class="text-3xl mb-1.5">{statusIcon(health.auditStatus.status)}</div>
        <div class="font-bold text-base dark:text-white">سلسلة التدقيق</div>
        <div class="text-xs mt-1 dark:text-gray-300">{health.auditStatus.message}</div>
      </div>
      <div class="rounded-xl p-5 shadow text-center border-2 hover:-translate-y-0.5 transition-transform {statusClass(health.syncStatus.status)}">
        <div class="text-3xl mb-1.5">{statusIcon(health.syncStatus.status)}</div>
        <div class="font-bold text-base dark:text-white">المزامنة</div>
        <div class="text-xs mt-1 dark:text-gray-300">{health.syncStatus.message}</div>
      </div>
    </div>

    <!-- Metrics -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
      <!-- Backup Details -->
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow">
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3">💾 تفاصيل النسخ الاحتياطية</h2>
        <div class="flex flex-col gap-2">
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">عدد النسخ</span><strong class="text-gray-800 dark:text-white">{health.backupStatus.backupCount}</strong></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر نسخة</span><strong class="text-gray-800 dark:text-white">{fmt(health.backupStatus.lastBackup)}</strong></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">عمر آخر نسخة</span>
            <strong class="text-gray-800 dark:text-white">{health.backupStatus.backupAgeDays !== null ? health.backupStatus.backupAgeDays + ' يوم' : '—'}</strong>
          </div>
        </div>
      </div>

      <!-- Sync Details -->
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow">
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🔄 تفاصيل المزامنة</h2>
        <div class="flex flex-col gap-2">
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">استيرادات فاشلة</span><strong class="text-gray-800 dark:text-white {health.syncStatus.failedImportsCount>0?'text-red-600 dark:text-red-400':''}">{health.syncStatus.failedImportsCount}</strong></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">تعارضات غير محلولة</span><strong class="text-gray-800 dark:text-white {health.syncStatus.unresolvedConflicts>0?'text-red-600 dark:text-red-400':''}">{health.syncStatus.unresolvedConflicts}</strong></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">حزم مكررة</span><strong class="text-gray-800 dark:text-white">{health.syncStatus.duplicatePackageAttempts}</strong></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر مزامنة ناجحة</span><strong class="text-gray-800 dark:text-white">{fmt(health.syncStatus.lastSuccessfulSync)}</strong></div>
        </div>
      </div>

      <!-- Storage -->
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow">
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
      </div>
    </div>

    <!-- Node Table -->
    {#if nodes.length > 0}
      <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🌐 صحة العقد المزامنة ({nodes.length})</h2>
      <div class="bg-white dark:bg-gray-800 rounded-xl shadow overflow-hidden mb-6">
        <table class="w-full border-collapse">
          <thead>
            <tr>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">الحالة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">معرف العقدة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">آخر مزامنة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">حزم مستلمة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">حزم مرفوضة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">إعادة تشغيل</th>
            </tr>
          </thead>
          <tbody>
            {#each nodes as n}
              <tr>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700"><span class="px-2 py-1 rounded text-xs font-semibold border {statusClass(n.status)}">{statusIcon(n.status)} {n.status}</span></td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700"><code class="dark:text-gray-300">{n.nodeId}</code></td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 dark:text-gray-300">{fmt(n.lastSyncTimestamp)}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 dark:text-gray-300">{n.packagesReceived}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 {n.packagesRejected>0?'text-red-600 dark:text-red-400':'dark:text-gray-300'}">{n.packagesRejected}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 {n.replayAttempts>0?'text-orange-600 dark:text-orange-400':'dark:text-gray-300'}">{n.replayAttempts}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="text-center p-8 bg-gray-50 dark:bg-gray-900/50 rounded-xl text-gray-400 mb-6">لا توجد بيانات لعقد المزامنة بعد</div>
    {/if}

    <!-- Operational Telemetry -->
    {#if telemetry.length > 0}
      <h2 class="font-semibold text-gray-800 dark:text-white mb-3 mt-6">📊 السجلات التشغيلية (Operational Telemetry)</h2>
      <div class="bg-white dark:bg-gray-800 rounded-xl shadow overflow-hidden mb-6">
        <table class="w-full border-collapse">
          <thead>
            <tr>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">التوقيت</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">العملية</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">النتيجة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">المدة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">المستخدم</th>
            </tr>
          </thead>
          <tbody>
            {#each telemetry as t}
              <tr>
                <td class="px-4 py-3 text-xs border-b border-gray-100 dark:border-gray-700 dark:text-gray-400">{fmt(t.timestamp)}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700"><code class="text-xs dark:text-gray-300">{t.event_type}</code></td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700"><span class="px-2 py-1 rounded text-xs font-semibold {t.outcome==='SUCCESS'?'bg-green-50 text-green-600 dark:bg-green-900/20 dark:text-green-400':'bg-red-50 text-red-600 dark:bg-red-900/20 dark:text-red-400'}">{t.outcome}</span></td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 dark:text-gray-300">{t.duration_ms !== null ? t.duration_ms + ' ms' : '—'}</td>
                <td class="px-4 py-3 text-xs border-b border-gray-100 dark:border-gray-700 dark:text-gray-400">{t.user_id || 'System'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    <!-- Build Metadata -->

    {#if buildInfo}
      <div class="bg-slate-50 dark:bg-gray-900 border border-slate-200 dark:border-gray-700 rounded-xl p-5 shadow mt-6">
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🛠️ معلومات الإصدار والبناء (Build Information)</h2>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">إصدار التطبيق</span><code class="dark:text-gray-300">{buildInfo.app_version}</code></div>
          <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">معرف الالتزام (Git)</span><code class="dark:text-gray-300">{buildInfo.git_commit}</code></div>
          <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">إصدار المخطط</span><code class="dark:text-gray-300">v{buildInfo.schema_version}</code></div>
          <div class="flex flex-col gap-1 p-2 text-sm"><span class="text-gray-500 dark:text-gray-400 font-semibold">تاريخ البناء</span><span class="text-xs text-slate-500 dark:text-slate-400">{fmt(new Date(parseInt(buildInfo.build_timestamp)*1000).toISOString())}</span></div>
        </div>
      </div>
    {/if}
  {/if}
</div>
</Layout>
