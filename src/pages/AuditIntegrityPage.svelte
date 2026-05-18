<script lang="ts">
  import { onMount } from 'svelte';
  import { getAuditHealth, getAuditChainStatus, getSettings } from '../lib/tauri';
  import type { AuditHealthReport, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let health: AuditHealthReport | null = null;
  let loading = true;
  let error: string | null = null;
  let settings: Settings | null = null;

  async function loadHealth() {
    loading = true; error = null;
    try { health = await getAuditHealth(); }
    catch (e) { error = e instanceof Error ? e.message : 'فشل التحميل'; }
    finally { loading = false; }
  }

  function fmt(ts: string | null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }
  function truncHash(h: string | null) { return h ? h.slice(0,20)+'...' : '—'; }
  function sevColor(s: string) {
    return s==='CRITICAL'?'bg-red-50 dark:bg-red-900/20 border-red-300 dark:border-red-800 text-red-600 dark:text-red-400':s==='ERROR'?'bg-orange-50 dark:bg-orange-900/20 border-orange-300 dark:border-orange-800 text-orange-600 dark:text-orange-400':s==='WARNING'?'bg-yellow-50 dark:bg-yellow-900/20 border-yellow-300 dark:border-yellow-800 text-yellow-600 dark:text-yellow-400':'bg-blue-50 dark:bg-blue-900/20 border-blue-300 dark:border-blue-800 text-blue-600 dark:text-blue-400';
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
  <div class="flex justify-between items-start mb-6">
    <div>
      <h1 class="text-2xl font-bold text-gray-800 dark:text-white">🔐 لوحة سلامة التدقيق</h1>
      <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">مراقبة وتحقق من سلسلة Hash التدقيق</p>
    </div>
    <button class="bg-blue-50 dark:bg-blue-900/200 hover:bg-blue-600 disabled:opacity-60 text-white px-5 py-2 rounded-lg transition-colors" on:click={loadHealth} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 text-red-600 dark:text-red-400 px-4 py-3 rounded-lg mb-4">{error}</div>{/if}

  {#if loading && !health}
    <div class="text-center p-12 bg-white dark:bg-gray-800 rounded-xl shadow">جارٍ التحميل...</div>
  {:else if health}
    <!-- Chain Banner -->
    <div class="flex items-center gap-4 p-5 rounded-xl mb-6 border-2 {valid ? 'bg-green-50 dark:bg-green-900/20 border-green-300 dark:border-green-800' : 'bg-red-50 dark:bg-red-900/20 border-red-300 dark:border-red-800'}">
      <span class="text-3xl">{valid ? '✅' : '🚨'}</span>
      <div>
        <div class="font-bold text-lg dark:text-white">{valid ? 'سلسلة التدقيق سليمة' : 'تحذير: الإخلال في سلسلة التدقيق!'}</div>
        <div class="text-sm text-gray-500 dark:text-gray-400">{health.chainStatus.verifiedEntries.toLocaleString()} سجل مُتحقق من {health.chainStatus.totalEntries.toLocaleString()}</div>
      </div>
    </div>

    <!-- Metrics -->
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow border-t-4 border-blue-500"><div class="text-2xl mb-2">📊</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{health.chainStatus.totalEntries.toLocaleString()}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">إجمالي السجلات</div></div>
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow border-t-4 border-green-500"><div class="text-2xl mb-2">🔗</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{health.chainStatus.verifiedEntries.toLocaleString()}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">سجلات مُتحققة</div></div>
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow border-t-4 border-purple-500"><div class="text-2xl mb-2">🔑</div><div class="text-2xl font-bold text-gray-800 dark:text-white font-mono text-sm">{truncHash(health.chainStatus.latestHash)}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">آخر Hash</div></div>
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow border-t-4 border-amber-500"><div class="text-2xl mb-2">⚠️</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{health.anomalies.length}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">شذوذات</div></div>
    </div>

    <!-- Info -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow border-t-4 border-gray-200 dark:border-gray-700">
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3">📅 آخر عمليات التزامن</h2>
        <div class="flex flex-col gap-2">
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر استيراد</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastImportTimestamp)}</span></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر تصدير</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastExportTimestamp)}</span></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر نسخة احتياطية</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastBackupTimestamp)}</span></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر استعادة</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.lastRestoreTimestamp)}</span></div>
          <div class="flex justify-between py-1.5 border-b border-gray-100 dark:border-gray-700 text-sm"><span class="text-gray-500 dark:text-gray-400">آخر إدخال</span><span class="font-medium text-gray-800 dark:text-white">{fmt(health.chainStatus.latestEntryTimestamp)}</span></div>
        </div>
      </div>
      <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow border-t-4 border-gray-200 dark:border-gray-700">
        <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🔒 Hash الأخير</h2>
        {#if health.chainStatus.latestHash}
          <div class="bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg p-3 break-all"><code class="font-mono text-xs text-gray-800 dark:text-gray-300">{health.chainStatus.latestHash}</code></div>
        {:else}
          <p class="text-gray-400 dark:text-gray-500 dark:text-gray-400 italic">لا توجد سجلات مُجزأة</p>
        {/if}
        {#if !valid && health.chainStatus.breakDescription}
          <div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 text-red-600 dark:text-red-400 px-4 py-3 rounded-lg mt-3">{health.chainStatus.breakDescription}</div>
        {/if}
      </div>
    </div>

    <!-- Anomalies -->
    {#if health.anomalies.length > 0}
      <h2 class="font-semibold text-gray-800 dark:text-white mb-3">⚠️ الشذوذات ({health.anomalies.length})</h2>
      {#each health.anomalies as a}
        <div class="p-3 rounded-lg mb-2 border {sevColor(a.severity)}">
          <div class="flex gap-3 items-center mb-1.5 flex-wrap"><strong>{a.severity}</strong><code>{a.anomalyType}</code><span class="text-xs opacity-70 mr-auto">{fmt(a.detectedAt)}</span></div>
          <p>{a.description}</p>
        </div>
      {/each}
    {:else}
      <div class="text-center p-8 bg-green-50 dark:bg-green-900/20 border border-green-300 dark:border-green-800 rounded-xl text-green-600 dark:text-green-400 font-semibold">✅ لا توجد شذوذات — النظام يعمل بشكل طبيعي</div>
    {/if}
  {/if}
</div>
</Layout>
