<script lang="ts">
  import { onMount } from 'svelte';
  import { getSyncHealth, getConflictSummary, getSettings } from '../lib/tauri';
  import type { SyncNodeHealth, ConflictSummary, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let nodes: SyncNodeHealth[] = [];
  let summary: ConflictSummary | null = null;
  let loading = true;
  let error: string | null = null;
  let settings: Settings | null = null;

  async function load() {
    loading = true; error = null;
    try {
      [nodes, summary] = await Promise.all([getSyncHealth(), getConflictSummary()]);
    } catch(e) { error = e instanceof Error ? e.message : 'فشل التحميل'; }
    finally { loading = false; }
  }

  function fmt(ts: string|null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }
  function stIcon(s: string) { return s==='HEALTHY'?'✅':s==='DEGRADED'?'⚠️':s==='CRITICAL'?'🔴':'❓'; }
  function stCls(s: string) { return s==='HEALTHY'?'n-ok':s==='DEGRADED'?'n-warn':'n-crit'; }

  // Simple canvas-based node graph
  let canvas: HTMLCanvasElement;
  $: if (canvas && nodes.length > 0) drawGraph();

  function drawGraph() {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H);

    // Central hub
    const cx = W/2, cy = H/2;
    const r = Math.min(W, H) * 0.32;

    // Draw central node
    ctx.beginPath();
    ctx.arc(cx, cy, 28, 0, Math.PI*2);
    ctx.fillStyle = '#1e293b';
    ctx.fill();
    ctx.fillStyle = '#fff';
    ctx.font = 'bold 11px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('WILAYA', cx, cy);

    // Draw peripheral nodes
    nodes.slice(0, 8).forEach((n, i) => {
      const angle = (i / Math.min(nodes.length, 8)) * Math.PI * 2 - Math.PI/2;
      const nx = cx + r * Math.cos(angle);
      const ny = cy + r * Math.sin(angle);

      // Line
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(nx, ny);
      ctx.strokeStyle = n.status==='HEALTHY'?'#86efac':n.status==='DEGRADED'?'#fde047':'#fca5a5';
      ctx.lineWidth = 2;
      ctx.stroke();

      // Node circle
      ctx.beginPath();
      ctx.arc(nx, ny, 20, 0, Math.PI*2);
      ctx.fillStyle = n.status==='HEALTHY'?'#10b981':n.status==='DEGRADED'?'#f59e0b':'#ef4444';
      ctx.fill();

      // Label
      ctx.fillStyle = '#fff';
      ctx.font = '9px sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      const label = n.nodeId.slice(0, 6);
      ctx.fillText(label, nx, ny);
    });
  }

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await load();
  });

  $: nodeType = settings?.node_type || null;
</script>

<Layout {nodeType} title="طوبولوجيا المزامنة" subtitle="مراقبة العقد والعلاقات">
<div class="p" dir="rtl">
  <div class="hdr">
    <div>
      <h1 class="t1">🌐 طوبولوجيا المزامنة</h1>
      <p class="t2">خريطة العقد والحزم والصحة التشغيلية</p>
    </div>
    <button class="btn" on:click={load} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="err">{error}</div>{/if}

  {#if loading && !summary}
    <div class="ldg">جارٍ تحميل بيانات الشبكة...</div>
  {:else}
    <!-- Summary Cards -->
    {#if summary}
    <div class="grid4">
      <div class="card c-blue"><div class="ci">📦</div><div class="cv">{summary.total}</div><div class="cl">إجمالي التعارضات</div></div>
      <div class="card c-red"><div class="ci">🔴</div><div class="cv">{summary.unresolved}</div><div class="cl">غير محلول</div></div>
      <div class="card c-green"><div class="ci">🌐</div><div class="cv">{nodes.length}</div><div class="cl">عقد نشطة</div></div>
      <div class="card c-purple"><div class="ci">✅</div><div class="cv">{summary.total - summary.unresolved}</div><div class="cl">تعارضات محلولة</div></div>
    </div>
    {/if}

    <div class="main-grid">
      <!-- Node Graph -->
      <div class="card">
        <h2 class="ct">🗺️ خريطة العقد</h2>
        {#if nodes.length > 0}
          <canvas bind:this={canvas} width="400" height="320" class="graph-canvas"></canvas>
          <p class="graph-hint">أخضر = سليم &nbsp;|&nbsp; أصفر = متدهور &nbsp;|&nbsp; أحمر = حرج</p>
        {:else}
          <div class="no-data">لا توجد بيانات عقد بعد. ستظهر هنا عند أول مزامنة.</div>
        {/if}
      </div>

      <!-- Type Breakdown -->
      <div>
        {#if summary && summary.byType.length > 0}
        <div class="card mb">
          <h2 class="ct">📊 التعارضات حسب النوع</h2>
          {#each summary.byType as t}
            <div class="type-row">
              <span class="type-lbl">{t.conflictTypeDisplay}</span>
              <div class="bar-wrap">
                <div class="bar" style="width:{Math.min((t.count/summary.total)*100,100)}%"></div>
              </div>
              <span class="type-cnt">{t.count}</span>
            </div>
          {/each}
        </div>
        {/if}

        {#if summary && summary.bySeverity.length > 0}
        <div class="card">
          <h2 class="ct">⚠️ التعارضات حسب الشدة</h2>
          {#each summary.bySeverity as s}
            <div class="type-row">
              <span class="type-lbl">{s.severity}</span>
              <div class="bar-wrap">
                <div class="bar bar-sev-{s.severity.toLowerCase()}" style="width:{Math.min((s.count/summary.total)*100,100)}%"></div>
              </div>
              <span class="type-cnt">{s.count}</span>
            </div>
          {/each}
        </div>
        {/if}
      </div>
    </div>

    <!-- Node Health Table -->
    {#if nodes.length > 0}
    <h2 class="st">📋 تفاصيل العقد ({nodes.length})</h2>
    <div class="tbl-wrap">
      <table class="tbl">
        <thead><tr>
          <th>الحالة</th><th>معرف العقدة</th><th>آخر مزامنة</th>
          <th>مستلم</th><th>مرفوض</th><th>إعادة تشغيل</th>
        </tr></thead>
        <tbody>
          {#each nodes as n}
          <tr class="{stCls(n.status)}">
            <td>{stIcon(n.status)} {n.status}</td>
            <td><code>{n.nodeId}</code></td>
            <td>{fmt(n.lastSyncTimestamp)}</td>
            <td>{n.packagesReceived}</td>
            <td class="{n.packagesRejected>0?'red':''}">{n.packagesRejected}</td>
            <td class="{n.replayAttempts>0?'orange':''}">{n.replayAttempts}</td>
          </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {/if}
  {/if}
</div>
</Layout>

<style>
  .p{padding:1.5rem;max-width:1200px;margin:0 auto}
  .hdr{display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:1.5rem}
  .t1{font-size:1.6rem;font-weight:700;color:#1e293b}
  .t2{color:#64748b;font-size:.85rem;margin-top:.2rem}
  .btn{background:#3b82f6;color:#fff;border:none;padding:.6rem 1.2rem;border-radius:.5rem;cursor:pointer}
  .btn:hover:not(:disabled){background:#2563eb}.btn:disabled{opacity:.6}
  .err{background:#fef2f2;border:1px solid #fca5a5;color:#dc2626;padding:.75rem;border-radius:.5rem;margin-bottom:1rem}
  .ldg{text-align:center;padding:3rem;background:#fff;border-radius:.75rem}
  .grid4{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:1rem;margin-bottom:1.5rem}
  .card{background:#fff;border-radius:.75rem;padding:1.25rem;box-shadow:0 1px 3px rgba(0,0,0,.08);border-top:4px solid #e2e8f0}
  .c-blue{border-top-color:#3b82f6}.c-red{border-top-color:#ef4444}.c-green{border-top-color:#10b981}.c-purple{border-top-color:#8b5cf6}
  .ci{font-size:1.5rem;margin-bottom:.4rem}
  .cv{font-size:1.4rem;font-weight:700;color:#1e293b}
  .cl{font-size:.78rem;color:#64748b;margin-top:.2rem}
  .main-grid{display:grid;grid-template-columns:1fr 1fr;gap:1rem;margin-bottom:1.5rem}
  @media(max-width:768px){.main-grid{grid-template-columns:1fr}}
  .ct{font-size:1rem;font-weight:600;color:#1e293b;margin-bottom:.75rem}
  .graph-canvas{width:100%;max-width:400px;display:block;margin:0 auto}
  .graph-hint{text-align:center;font-size:.75rem;color:#94a3b8;margin-top:.5rem}
  .no-data{text-align:center;padding:2rem;color:#94a3b8;font-style:italic}
  .mb{margin-bottom:1rem}
  .type-row{display:flex;align-items:center;gap:.5rem;margin-bottom:.4rem;font-size:.85rem}
  .type-lbl{min-width:160px;color:#64748b}
  .bar-wrap{flex:1;height:8px;background:#e2e8f0;border-radius:4px;overflow:hidden}
  .bar{height:100%;background:linear-gradient(90deg,#3b82f6,#8b5cf6);border-radius:4px;transition:width .5s}
  .bar-sev-critical{background:#ef4444}.bar-sev-error{background:#f97316}.bar-sev-warning{background:#eab308}.bar-sev-info{background:#3b82f6}
  .type-cnt{min-width:30px;text-align:left;font-weight:600;color:#1e293b}
  .st{font-size:1rem;font-weight:600;color:#1e293b;margin-bottom:.75rem}
  .tbl-wrap{background:#fff;border-radius:.75rem;box-shadow:0 1px 3px rgba(0,0,0,.08);overflow:hidden;margin-bottom:1.5rem}
  .tbl{width:100%;border-collapse:collapse}
  .tbl th{background:#f8fafc;padding:.75rem 1rem;text-align:right;font-size:.8rem;font-weight:600;color:#64748b;border-bottom:1px solid #e2e8f0}
  .tbl td{padding:.7rem 1rem;font-size:.88rem;border-bottom:1px solid #f1f5f9}
  .n-warn td:first-child{color:#ca8a04}.n-crit td:first-child{color:#dc2626}
  .red{color:#dc2626}.orange{color:#ea580c}
</style>
