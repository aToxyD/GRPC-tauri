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
    return s==='HEALTHY'?'st-ok':s==='DEGRADED'?'st-warn':s==='CRITICAL'?'st-crit':'st-unk';
  }

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await load();
  });

  $: nodeType = (settings?.node_type === 'WILAYA' ? 'WILAYA' : settings?.node_type === 'UNIT' ? 'UNIT' : null) as 'WILAYA' | 'UNIT' | null;
</script>

<Layout {nodeType} title="صحة النظام" subtitle="لوحة المراقبة التشغيلية الشاملة">
<div class="p" dir="rtl">
  <div class="hdr">
    <div>
      <h1 class="t1">🖥️ صحة النظام</h1>
      <p class="t2">المراقبة التشغيلية الشاملة للعقدة</p>
    </div>
    <button class="btn" on:click={load} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="err">{error}</div>{/if}

  {#if loading && !health}
    <div class="ldg">جارٍ تحميل بيانات صحة النظام...</div>
  {:else if health}
    <!-- Status Cards -->
    <div class="bg-blue-50 border border-blue-200 p-4 rounded-xl mb-6 text-sm text-blue-800">
      <h3 class="font-bold mb-1">دليل حالات النظام (Integrity States):</h3>
      <ul class="list-disc pr-5 space-y-1">
        <li><strong>✅ HEALTHY:</strong> النظام سليم ولا توجد مشاكل.</li>
        <li><strong>⚠️ DEGRADED / WARNING:</strong> هناك تعارضات أو مشاكل طفيفة يجب مراجعتها، لكن العمليات الأساسية مستمرة.</li>
        <li><strong>🔴 CRITICAL / CORRUPTED:</strong> تم اكتشاف تلف أو تلاعب. النظام في حالة حماية (Fail-Closed). يُمنع إدخال بيانات جديدة ويجب الاستعانة بنسخة احتياطية.</li>
      </ul>
    </div>
    <div class="grid4">
      <div class="scard {statusClass(health.databaseStatus.status)}">
        <div class="sico">{statusIcon(health.databaseStatus.status)}</div>
        <div class="sname">قاعدة البيانات</div>
        <div class="smsg">{health.databaseStatus.message}</div>
      </div>
      <div class="scard {statusClass(health.backupStatus.status)}">
        <div class="sico">{statusIcon(health.backupStatus.status)}</div>
        <div class="sname">النسخ الاحتياطية</div>
        <div class="smsg">{health.backupStatus.message}</div>
      </div>
      <div class="scard {statusClass(health.auditStatus.status)}">
        <div class="sico">{statusIcon(health.auditStatus.status)}</div>
        <div class="sname">سلسلة التدقيق</div>
        <div class="smsg">{health.auditStatus.message}</div>
      </div>
      <div class="scard {statusClass(health.syncStatus.status)}">
        <div class="sico">{statusIcon(health.syncStatus.status)}</div>
        <div class="sname">المزامنة</div>
        <div class="smsg">{health.syncStatus.message}</div>
      </div>
    </div>

    <!-- Metrics -->
    <div class="grid3">
      <!-- Backup Details -->
      <div class="card">
        <h2 class="ct">💾 تفاصيل النسخ الاحتياطية</h2>
        <div class="rows">
          <div class="row"><span>عدد النسخ</span><strong>{health.backupStatus.backupCount}</strong></div>
          <div class="row"><span>آخر نسخة</span><strong>{fmt(health.backupStatus.lastBackup)}</strong></div>
          <div class="row"><span>عمر آخر نسخة</span>
            <strong>{health.backupStatus.backupAgeDays !== null ? health.backupStatus.backupAgeDays + ' يوم' : '—'}</strong>
          </div>
        </div>
      </div>

      <!-- Sync Details -->
      <div class="card">
        <h2 class="ct">🔄 تفاصيل المزامنة</h2>
        <div class="rows">
          <div class="row"><span>استيرادات فاشلة</span><strong class="{health.syncStatus.failedImportsCount>0?'c-red':''}">{health.syncStatus.failedImportsCount}</strong></div>
          <div class="row"><span>تعارضات غير محلولة</span><strong class="{health.syncStatus.unresolvedConflicts>0?'c-red':''}">{health.syncStatus.unresolvedConflicts}</strong></div>
          <div class="row"><span>حزم مكررة</span><strong>{health.syncStatus.duplicatePackageAttempts}</strong></div>
          <div class="row"><span>آخر مزامنة ناجحة</span><strong>{fmt(health.syncStatus.lastSuccessfulSync)}</strong></div>
        </div>
      </div>

      <!-- Storage -->
      <div class="card">
        <h2 class="ct">🗄️ التخزين والذاكرة</h2>
        <div class="rows">
          <div class="row"><span>حجم قاعدة البيانات</span><strong>{fmtBytes(health.storageUsageBytes)}</strong></div>
          <div class="row"><span>تاريخ التقرير</span><strong>{fmt(health.generatedAt)}</strong></div>
        </div>
        <!-- Storage bar -->
        <div class="stor-bar-wrap">
          <div class="stor-bar" style="width:{Math.min((health.storageUsageBytes/10485760)*100,100)}%"></div>
        </div>
        <div class="stor-lbl">{fmtBytes(health.storageUsageBytes)} / 10 MB (تقدير)</div>
      </div>
    </div>

    <!-- Node Table -->
    {#if nodes.length > 0}
      <h2 class="st">🌐 صحة العقد المزامنة ({nodes.length})</h2>
      <div class="tbl-wrap">
        <table class="tbl">
          <thead>
            <tr>
              <th>الحالة</th>
              <th>معرف العقدة</th>
              <th>آخر مزامنة</th>
              <th>حزم مستلمة</th>
              <th>حزم مرفوضة</th>
              <th>إعادة تشغيل</th>
            </tr>
          </thead>
          <tbody>
            {#each nodes as n}
              <tr>
                <td><span class="badge {statusClass(n.status)}">{statusIcon(n.status)} {n.status}</span></td>
                <td><code>{n.nodeId}</code></td>
                <td>{fmt(n.lastSyncTimestamp)}</td>
                <td>{n.packagesReceived}</td>
                <td class="{n.packagesRejected>0?'c-red':''}">{n.packagesRejected}</td>
                <td class="{n.replayAttempts>0?'c-orange':''}">{n.replayAttempts}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="no-nodes">لا توجد بيانات لعقد المزامنة بعد</div>
    {/if}

    <!-- Operational Telemetry -->
    {#if telemetry.length > 0}
      <h2 class="st mt-6">📊 السجلات التشغيلية (Operational Telemetry)</h2>
      <div class="tbl-wrap">
        <table class="tbl">
          <thead>
            <tr>
              <th>التوقيت</th>
              <th>العملية</th>
              <th>النتيجة</th>
              <th>المدة</th>
              <th>المستخدم</th>
            </tr>
          </thead>
          <tbody>
            {#each telemetry as t}
              <tr>
                <td class="text-xs">{fmt(t.timestamp)}</td>
                <td><code class="text-xs">{t.event_type}</code></td>
                <td><span class="badge {t.outcome==='SUCCESS'?'st-ok':'st-crit'}">{t.outcome}</span></td>
                <td>{t.duration_ms !== null ? t.duration_ms + ' ms' : '—'}</td>
                <td class="text-xs">{t.user_id || 'System'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    <!-- Build Metadata -->

    {#if buildInfo}
      <div class="card mt-6" style="background: #f8fafc; border: 1px solid #e2e8f0;">
        <h2 class="st">🛠️ معلومات الإصدار والبناء (Build Information)</h2>
        <div class="grid4" style="margin-bottom: 0;">
          <div class="b-row"><span>إصدار التطبيق</span><code>{buildInfo.app_version}</code></div>
          <div class="b-row"><span>معرف الالتزام (Git)</span><code>{buildInfo.git_commit}</code></div>
          <div class="b-row"><span>إصدار المخطط</span><code>v{buildInfo.schema_version}</code></div>
          <div class="b-row"><span>تاريخ البناء</span><span class="text-xs text-slate-500">{fmt(new Date(parseInt(buildInfo.build_timestamp)*1000).toISOString())}</span></div>
        </div>
      </div>
    {/if}
  {/if}
</div>
</Layout>

<style>
  .b-row{display:flex;flex-direction:column;gap:0.25rem;padding:0.5rem;font-size:0.85rem}
  .b-row span{color:#64748b;font-weight:600}
  .mt-6{margin-top:1.5rem}

  .p{padding:1.5rem;max-width:1200px;margin:0 auto}
  .hdr{display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:1.5rem}
  .t1{font-size:1.6rem;font-weight:700;color:#1e293b}
  .t2{color:#64748b;font-size:.85rem;margin-top:.2rem}
  .btn{background:#3b82f6;color:#fff;border:none;padding:.6rem 1.2rem;border-radius:.5rem;cursor:pointer}
  .btn:hover:not(:disabled){background:#2563eb}.btn:disabled{opacity:.6}
  .err{background:#fef2f2;border:1px solid #fca5a5;color:#dc2626;padding:.75rem;border-radius:.5rem;margin-bottom:1rem}
  .ldg{text-align:center;padding:3rem;background:#fff;border-radius:.75rem}
  .grid4{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:1rem;margin-bottom:1.5rem}
  .grid3{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem;margin-bottom:1.5rem}
  .scard{background:#fff;border-radius:.75rem;padding:1.25rem;box-shadow:0 1px 3px rgba(0,0,0,.08);text-align:center;border:2px solid;transition:transform .2s}
  .scard:hover{transform:translateY(-2px)}
  .st-ok{border-color:#86efac;background:#f0fdf4}.st-warn{border-color:#fde047;background:#fefce8}
  .st-crit{border-color:#fca5a5;background:#fef2f2}.st-unk{border-color:#e2e8f0;background:#f8fafc}
  .sico{font-size:2rem;margin-bottom:.4rem}
  .sname{font-weight:700;font-size:1rem;color:#1e293b}
  .smsg{font-size:.78rem;color:#64748b;margin-top:.25rem}
  .card{background:#fff;border-radius:.75rem;padding:1.25rem;box-shadow:0 1px 3px rgba(0,0,0,.08)}
  .ct{font-size:1rem;font-weight:600;color:#1e293b;margin-bottom:.75rem}
  .rows{display:flex;flex-direction:column;gap:.5rem}
  .row{display:flex;justify-content:space-between;padding:.4rem 0;border-bottom:1px solid #f1f5f9;font-size:.88rem}
  .row span{color:#64748b}.row strong{color:#1e293b}
  .c-red{color:#dc2626!important}.c-orange{color:#ea580c!important}
  .stor-bar-wrap{height:8px;background:#e2e8f0;border-radius:4px;margin-top:1rem;overflow:hidden}
  .stor-bar{height:100%;background:linear-gradient(90deg,#3b82f6,#8b5cf6);border-radius:4px;transition:width .5s}
  .stor-lbl{font-size:.75rem;color:#64748b;margin-top:.25rem;text-align:center}
  .st{font-size:1rem;font-weight:600;color:#1e293b;margin-bottom:.75rem}
  .tbl-wrap{background:#fff;border-radius:.75rem;box-shadow:0 1px 3px rgba(0,0,0,.08);overflow:hidden;margin-bottom:1.5rem}
  .tbl{width:100%;border-collapse:collapse}
  .tbl th{background:#f8fafc;padding:.75rem 1rem;text-align:right;font-size:.8rem;font-weight:600;color:#64748b;border-bottom:1px solid #e2e8f0}
  .tbl td{padding:.75rem 1rem;font-size:.88rem;border-bottom:1px solid #f1f5f9}
  .tbl tr:last-child td{border-bottom:none}
  .badge{padding:.2rem .6rem;border-radius:.25rem;font-size:.78rem;font-weight:600}
  .no-nodes{text-align:center;padding:2rem;background:#f8fafc;border-radius:.75rem;color:#94a3b8}
</style>
