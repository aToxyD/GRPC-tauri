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
    return s==='CRITICAL'?'sev-critical':s==='ERROR'?'sev-error':s==='WARNING'?'sev-warning':'sev-info';
  }

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await loadHealth();
  });

  $: nodeType = (settings?.node_type === 'WILAYA' ? 'WILAYA' : settings?.node_type === 'UNIT' ? 'UNIT' : null) as 'WILAYA' | 'UNIT' | null;
  $: valid = health?.chainStatus?.isValid ?? null;
</script>

<Layout {nodeType} title="سلامة التدقيق" subtitle="مراقبة سلسلة التدقيق">
<div class="p" dir="rtl">
  <div class="hdr">
    <div>
      <h1 class="t1">🔐 لوحة سلامة التدقيق</h1>
      <p class="t2">مراقبة وتحقق من سلسلة Hash التدقيق</p>
    </div>
    <button class="btn-ref" on:click={loadHealth} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="err-box">{error}</div>{/if}

  {#if loading && !health}
    <div class="loading">جارٍ التحميل...</div>
  {:else if health}
    <!-- Chain Banner -->
    <div class="banner {valid ? 'banner-ok' : 'banner-bad'}">
      <span class="bicon">{valid ? '✅' : '🚨'}</span>
      <div>
        <div class="bst">{valid ? 'سلسلة التدقيق سليمة' : 'تحذير: الإخلال في سلسلة التدقيق!'}</div>
        <div class="bsub">{health.chainStatus.verifiedEntries.toLocaleString()} سجل مُتحقق من {health.chainStatus.totalEntries.toLocaleString()}</div>
      </div>
    </div>

    <!-- Metrics -->
    <div class="grid4">
      <div class="card c-blue"><div class="cicon">📊</div><div class="cval">{health.chainStatus.totalEntries.toLocaleString()}</div><div class="clbl">إجمالي السجلات</div></div>
      <div class="card c-green"><div class="cicon">🔗</div><div class="cval">{health.chainStatus.verifiedEntries.toLocaleString()}</div><div class="clbl">سجلات مُتحققة</div></div>
      <div class="card c-purple"><div class="cicon">🔑</div><div class="cval mono">{truncHash(health.chainStatus.latestHash)}</div><div class="clbl">آخر Hash</div></div>
      <div class="card c-amber"><div class="cicon">⚠️</div><div class="cval">{health.anomalies.length}</div><div class="clbl">شذوذات</div></div>
    </div>

    <!-- Info -->
    <div class="grid2">
      <div class="card">
        <h2 class="ct">📅 آخر عمليات التزامن</h2>
        <div class="rows">
          <div class="row"><span>آخر استيراد</span><span>{fmt(health.lastImportTimestamp)}</span></div>
          <div class="row"><span>آخر تصدير</span><span>{fmt(health.lastExportTimestamp)}</span></div>
          <div class="row"><span>آخر نسخة احتياطية</span><span>{fmt(health.lastBackupTimestamp)}</span></div>
          <div class="row"><span>آخر استعادة</span><span>{fmt(health.lastRestoreTimestamp)}</span></div>
          <div class="row"><span>آخر إدخال</span><span>{fmt(health.chainStatus.latestEntryTimestamp)}</span></div>
        </div>
      </div>
      <div class="card">
        <h2 class="ct">🔒 Hash الأخير</h2>
        {#if health.chainStatus.latestHash}
          <div class="hash-box"><code>{health.chainStatus.latestHash}</code></div>
        {:else}
          <p class="none">لا توجد سجلات مُجزأة</p>
        {/if}
        {#if !valid && health.chainStatus.breakDescription}
          <div class="err-box mt">{health.chainStatus.breakDescription}</div>
        {/if}
      </div>
    </div>

    <!-- Anomalies -->
    {#if health.anomalies.length > 0}
      <h2 class="st">⚠️ الشذوذات ({health.anomalies.length})</h2>
      {#each health.anomalies as a}
        <div class="anom {sevColor(a.severity)}">
          <div class="arow"><strong>{a.severity}</strong><code>{a.anomalyType}</code><span class="atm">{fmt(a.detectedAt)}</span></div>
          <p>{a.description}</p>
        </div>
      {/each}
    {:else}
      <div class="ok-box">✅ لا توجد شذوذات — النظام يعمل بشكل طبيعي</div>
    {/if}
  {/if}
</div>
</Layout>

<style>
  .p{padding:1.5rem;max-width:1100px;margin:0 auto}
  .hdr{display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:1.5rem}
  .t1{font-size:1.6rem;font-weight:700;color:#1e293b}
  .t2{color:#64748b;font-size:.85rem;margin-top:.2rem}
  .btn-ref{background:#3b82f6;color:#fff;border:none;padding:.6rem 1.2rem;border-radius:.5rem;cursor:pointer;transition:background .2s}
  .btn-ref:hover:not(:disabled){background:#2563eb}
  .btn-ref:disabled{opacity:.6;cursor:not-allowed}
  .err-box{background:#fef2f2;border:1px solid #fca5a5;color:#dc2626;padding:.75rem 1rem;border-radius:.5rem;margin-bottom:1rem}
  .loading{text-align:center;padding:3rem;background:#fff;border-radius:.75rem;box-shadow:0 1px 3px rgba(0,0,0,.1)}
  .banner{display:flex;align-items:center;gap:1rem;padding:1.25rem;border-radius:.75rem;margin-bottom:1.5rem;border:2px solid}
  .banner-ok{background:#f0fdf4;border-color:#86efac}
  .banner-bad{background:#fef2f2;border-color:#fca5a5}
  .bicon{font-size:2rem}
  .bst{font-weight:700;font-size:1.1rem}
  .bsub{font-size:.85rem;color:#64748b}
  .grid4{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:1rem;margin-bottom:1.5rem}
  .grid2{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:1rem;margin-bottom:1.5rem}
  .card{background:#fff;border-radius:.75rem;padding:1.25rem;box-shadow:0 1px 3px rgba(0,0,0,.08);border-top:4px solid #e2e8f0}
  .c-blue{border-top-color:#3b82f6}.c-green{border-top-color:#10b981}.c-purple{border-top-color:#8b5cf6}.c-amber{border-top-color:#f59e0b}
  .cicon{font-size:1.5rem;margin-bottom:.4rem}
  .cval{font-size:1.4rem;font-weight:700;color:#1e293b}
  .clbl{font-size:.78rem;color:#64748b;margin-top:.2rem}
  .mono{font-family:monospace;font-size:.85rem}
  .ct{font-size:1rem;font-weight:600;color:#1e293b;margin-bottom:.75rem}
  .rows{display:flex;flex-direction:column;gap:.5rem}
  .row{display:flex;justify-content:space-between;padding:.4rem 0;border-bottom:1px solid #f1f5f9;font-size:.88rem}
  .row span:first-child{color:#64748b}
  .row span:last-child{font-weight:500;color:#1e293b}
  .hash-box{background:#f8fafc;border:1px solid #e2e8f0;border-radius:.5rem;padding:.75rem;word-break:break-all}
  .hash-box code{font-family:monospace;font-size:.72rem;color:#1e293b}
  .none{color:#94a3b8;font-style:italic}
  .mt{margin-top:.75rem}
  .st{font-size:1rem;font-weight:600;color:#1e293b;margin-bottom:.75rem}
  .anom{padding:.75rem 1rem;border-radius:.5rem;margin-bottom:.5rem;border:1px solid}
  .sev-critical{background:#fef2f2;border-color:#fca5a5;color:#dc2626}
  .sev-error{background:#fff7ed;border-color:#fdba74;color:#ea580c}
  .sev-warning{background:#fefce8;border-color:#fde047;color:#ca8a04}
  .sev-info{background:#eff6ff;border-color:#bfdbfe;color:#2563eb}
  .arow{display:flex;gap:.75rem;align-items:center;margin-bottom:.4rem;flex-wrap:wrap}
  .atm{font-size:.78rem;color:#64748b;margin-right:auto}
  .ok-box{text-align:center;padding:2rem;background:#f0fdf4;border:1px solid #86efac;border-radius:.75rem;color:#16a34a;font-weight:600}
</style>
