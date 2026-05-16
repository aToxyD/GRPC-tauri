<script lang="ts">
  import { onMount } from 'svelte';
  import { listSyncConflicts, resolveSyncConflict, getConflictSummary, getSettings } from '../lib/tauri';
  import type { SyncConflict, ConflictSummary, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let conflicts: SyncConflict[] = [];
  let summary: ConflictSummary | null = null;
  let loading = true;
  let error: string | null = null;
  let settings: Settings | null = null;
  let showUnresolvedOnly = false;
  let filterType = '';
  let filterSeverity = '';
  let selectedConflict: SyncConflict | null = null;
  let resolveNote = '';
  let resolving = false;

  async function load() {
    loading = true; error = null;
    try {
      [conflicts, summary] = await Promise.all([
        listSyncConflicts(showUnresolvedOnly),
        getConflictSummary()
      ]);
    } catch(e) { error = e instanceof Error ? e.message : 'فشل التحميل'; }
    finally { loading = false; }
  }

  async function doResolve() {
    if (!selectedConflict || !resolveNote.trim()) return;
    resolving = true;
    try {
      await resolveSyncConflict(selectedConflict.id, resolveNote);
      selectedConflict = null;
      resolveNote = '';
      await load();
    } catch(e) {
      error = e instanceof Error ? e.message : 'فشل في حل التعارض';
    } finally { resolving = false; }
  }

  function fmt(ts: string|null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }
  function sevCls(s: string) {
    return s==='CRITICAL'?'sev-crit':s==='ERROR'?'sev-err':s==='WARNING'?'sev-warn':'sev-info';
  }
  function sevIcon(s: string) {
    return s==='CRITICAL'?'🔴':s==='ERROR'?'🟠':s==='WARNING'?'🟡':'🔵';
  }

  $: filtered = conflicts.filter(c => {
    if (filterType && c.conflictType !== filterType) return false;
    if (filterSeverity && c.severity !== filterSeverity) return false;
    return true;
  });

  onMount(async () => {
    try { settings = await getSettings(); } catch {}
    await load();
  });

  $: nodeType = settings?.node_type || null;
</script>

<Layout {nodeType} title="مركز التعارضات" subtitle="كشف وإدارة تعارضات المزامنة">
<div class="p" dir="rtl">
  <div class="hdr">
    <div>
      <h1 class="t1">⚡ مركز التعارضات</h1>
      <p class="t2">كشف وتحليل وحل تعارضات المزامنة</p>
    </div>
    <button class="btn" on:click={load} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="err">{error}</div>{/if}

  <!-- Summary -->
  {#if summary}
  <div class="grid4">
    <div class="card c-blue"><div class="ci">📊</div><div class="cv">{summary.total}</div><div class="cl">إجمالي التعارضات</div></div>
    <div class="card {summary.unresolved>0?'c-red':'c-green'}"><div class="ci">{summary.unresolved>0?'🔴':'✅'}</div><div class="cv">{summary.unresolved}</div><div class="cl">غير محلول</div></div>
    <div class="card c-green"><div class="ci">✅</div><div class="cv">{summary.total-summary.unresolved}</div><div class="cl">محلول</div></div>
    <div class="card c-amber"><div class="ci">⚡</div><div class="cv">{summary.recentConflicts.length}</div><div class="cl">أحدث التعارضات</div></div>
  </div>
  {/if}

  <!-- Filters -->
  <div class="filters">
    <label class="chk-wrap">
      <input type="checkbox" bind:checked={showUnresolvedOnly} on:change={load}/>
      عرض غير المحلولة فقط
    </label>
    <select bind:value={filterType} class="sel">
      <option value="">كل الأنواع</option>
      <option value="STALE_IMPORT">استيراد قديم</option>
      <option value="DUPLICATE_PACKAGE">حزمة مكررة</option>
      <option value="OUT_OF_ORDER_REPORT">خارج الترتيب</option>
      <option value="DIVERGENT_STOCK_STATE">تباين المخزون</option>
      <option value="REPLAY_ATTEMPT">إعادة تشغيل</option>
    </select>
    <select bind:value={filterSeverity} class="sel">
      <option value="">كل الشدد</option>
      <option value="CRITICAL">حرج</option>
      <option value="ERROR">خطأ</option>
      <option value="WARNING">تحذير</option>
      <option value="INFO">معلومة</option>
    </select>
    <span class="filter-cnt">{filtered.length} نتيجة</span>
  </div>

  <!-- Conflicts Table -->
  {#if loading && !conflicts.length}
    <div class="ldg">جارٍ تحميل التعارضات...</div>
  {:else if filtered.length === 0}
    <div class="empty">✅ لا توجد تعارضات تطابق الفلاتر المحددة</div>
  {:else}
    <div class="tbl-wrap">
      <table class="tbl">
        <thead><tr>
          <th>الشدة</th><th>النوع</th><th>معرف الحزمة</th>
          <th>العقدة المصدر</th><th>الوقت</th><th>الحالة</th><th>إجراء</th>
        </tr></thead>
        <tbody>
          {#each filtered as c}
          <tr class="{c.resolved?'resolved':''}">
            <td><span class="badge {sevCls(c.severity)}">{sevIcon(c.severity)} {c.severity}</span></td>
            <td><span class="type-tag">{c.conflictTypeDisplay}</span></td>
            <td><code class="pkg-id" title={c.packageId}>{c.packageId.slice(0,12)}...</code></td>
            <td><code>{c.sourceNodeId}</code></td>
            <td class="ts">{fmt(c.createdAt)}</td>
            <td>
              {#if c.resolved}
                <span class="resolved-badge">✅ محلول</span>
              {:else}
                <span class="unresolved-badge">⏳ معلق</span>
              {/if}
            </td>
            <td>
              {#if !c.resolved}
                <button class="btn-sm" on:click={() => { selectedConflict = c; resolveNote = ''; }}>
                  حل
                </button>
              {:else}
                <button class="btn-sm btn-view" on:click={() => selectedConflict = c}>
                  عرض
                </button>
              {/if}
            </td>
          </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<!-- Resolve Modal -->
{#if selectedConflict}
<div
  class="overlay"
  role="dialog"
  aria-modal="true"
  aria-label="تفاصيل التعارض"
  tabindex="-1"
  on:click|self={() => selectedConflict = null}
  on:keydown={(e) => e.key === 'Escape' && (selectedConflict = null)}
>
  <div class="modal" dir="rtl">
    <div class="modal-hdr">
      <h2>تفاصيل التعارض</h2>
      <button class="close" on:click={() => selectedConflict = null}>✕</button>
    </div>
    <div class="modal-body">
      <div class="detail-row"><span>المعرف</span><code>{selectedConflict.id}</code></div>
      <div class="detail-row"><span>النوع</span><strong>{selectedConflict.conflictTypeDisplay}</strong></div>
      <div class="detail-row"><span>الشدة</span><span class="badge {sevCls(selectedConflict.severity)}">{selectedConflict.severity}</span></div>
      <div class="detail-row"><span>معرف الحزمة</span><code>{selectedConflict.packageId}</code></div>
      <div class="detail-row"><span>العقدة المصدر</span><code>{selectedConflict.sourceNodeId}</code></div>
      <div class="detail-row"><span>الوصف</span><p class="desc">{selectedConflict.description}</p></div>
      {#if selectedConflict.suggestedResolution}
        <div class="suggestion">
          <div class="sug-title">💡 الاقتراح المقترح</div>
          <p>{selectedConflict.suggestedResolution.description}</p>
          <div class="sug-action">الإجراء: <strong>{selectedConflict.suggestedResolution.action}</strong></div>
        </div>
      {/if}
      {#if selectedConflict.resolved}
        <div class="resolved-info">
          <div class="res-title">✅ تم الحل بواسطة: {selectedConflict.resolvedBy}</div>
          <p>{selectedConflict.resolutionNote}</p>
          <div class="res-time">في {fmt(selectedConflict.resolvedAt)}</div>
        </div>
      {:else}
        <div class="resolve-form">
          <label class="form-lbl" for="resolve-note">ملاحظة الحل <span class="req">*</span></label>
          <textarea id="resolve-note" bind:value={resolveNote} class="textarea" rows="3" placeholder="اذكر سبب الحل وما تم اتخاذه..."></textarea>
          <button class="btn-resolve" on:click={doResolve} disabled={resolving || !resolveNote.trim()}>
            {resolving ? 'جارٍ الحفظ...' : '✅ تأكيد الحل'}
          </button>
        </div>
      {/if}
    </div>
  </div>
</div>
{/if}
</Layout>

<style>
  .p{padding:1.5rem;max-width:1200px;margin:0 auto}
  .hdr{display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:1.5rem}
  .t1{font-size:1.6rem;font-weight:700;color:#1e293b}
  .t2{color:#64748b;font-size:.85rem;margin-top:.2rem}
  .btn{background:#3b82f6;color:#fff;border:none;padding:.6rem 1.2rem;border-radius:.5rem;cursor:pointer}
  .btn:hover:not(:disabled){background:#2563eb}.btn:disabled{opacity:.6}
  .err{background:#fef2f2;border:1px solid #fca5a5;color:#dc2626;padding:.75rem;border-radius:.5rem;margin-bottom:1rem}
  .grid4{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:1rem;margin-bottom:1.5rem}
  .card{background:#fff;border-radius:.75rem;padding:1.25rem;box-shadow:0 1px 3px rgba(0,0,0,.08);text-align:center;border-top:4px solid #e2e8f0}
  .c-blue{border-top-color:#3b82f6}.c-red{border-top-color:#ef4444}.c-green{border-top-color:#10b981}.c-amber{border-top-color:#f59e0b}
  .ci{font-size:1.5rem;margin-bottom:.4rem}.cv{font-size:1.4rem;font-weight:700;color:#1e293b}.cl{font-size:.78rem;color:#64748b;margin-top:.2rem}
  .filters{display:flex;align-items:center;gap:1rem;flex-wrap:wrap;background:#fff;border-radius:.5rem;padding:.75rem 1rem;box-shadow:0 1px 3px rgba(0,0,0,.08);margin-bottom:1rem}
  .chk-wrap{display:flex;align-items:center;gap:.5rem;font-size:.88rem;cursor:pointer;color:#1e293b}
  .sel{padding:.4rem .75rem;border:1px solid #e2e8f0;border-radius:.4rem;font-size:.85rem;color:#1e293b;background:#fff}
  .filter-cnt{margin-right:auto;font-size:.85rem;color:#64748b}
  .ldg{text-align:center;padding:3rem;background:#fff;border-radius:.75rem}
  .empty{text-align:center;padding:2rem;background:#f0fdf4;border:1px solid #86efac;border-radius:.75rem;color:#16a34a;font-weight:600}
  .tbl-wrap{background:#fff;border-radius:.75rem;box-shadow:0 1px 3px rgba(0,0,0,.08);overflow:hidden;margin-bottom:1.5rem}
  .tbl{width:100%;border-collapse:collapse}
  .tbl th{background:#f8fafc;padding:.75rem 1rem;text-align:right;font-size:.78rem;font-weight:600;color:#64748b;border-bottom:1px solid #e2e8f0}
  .tbl td{padding:.65rem 1rem;font-size:.85rem;border-bottom:1px solid #f1f5f9;vertical-align:middle}
  .resolved td{opacity:.65}
  .badge{padding:.2rem .5rem;border-radius:.25rem;font-size:.75rem;font-weight:700}
  .sev-crit{background:#fef2f2;color:#dc2626;border:1px solid #fca5a5}
  .sev-err{background:#fff7ed;color:#ea580c;border:1px solid #fdba74}
  .sev-warn{background:#fefce8;color:#ca8a04;border:1px solid #fde047}
  .sev-info{background:#eff6ff;color:#2563eb;border:1px solid #bfdbfe}
  .type-tag{background:#f1f5f9;color:#475569;padding:.15rem .5rem;border-radius:.25rem;font-size:.78rem}
  .pkg-id{font-family:monospace;font-size:.78rem;color:#64748b}
  .ts{font-size:.78rem;color:#64748b}
  .resolved-badge{color:#16a34a;font-size:.8rem;font-weight:600}
  .unresolved-badge{color:#d97706;font-size:.8rem;font-weight:600}
  .btn-sm{background:#3b82f6;color:#fff;border:none;padding:.3rem .75rem;border-radius:.35rem;cursor:pointer;font-size:.8rem}
  .btn-sm:hover{background:#2563eb}
  .btn-view{background:#64748b}
  .btn-view:hover{background:#475569}
  .overlay{position:fixed;inset:0;background:rgba(0,0,0,.5);display:flex;align-items:center;justify-content:center;z-index:100}
  .modal{background:#fff;border-radius:1rem;max-width:580px;width:90%;max-height:85vh;overflow-y:auto;box-shadow:0 20px 40px rgba(0,0,0,.15)}
  .modal-hdr{display:flex;justify-content:space-between;align-items:center;padding:1rem 1.5rem;border-bottom:1px solid #e2e8f0}
  .modal-hdr h2{font-size:1.1rem;font-weight:700;color:#1e293b;margin:0}
  .close{background:none;border:none;font-size:1.2rem;cursor:pointer;color:#64748b}
  .close:hover{color:#1e293b}
  .modal-body{padding:1.5rem;display:flex;flex-direction:column;gap:.75rem}
  .detail-row{display:flex;align-items:baseline;gap:.5rem;font-size:.88rem}
  .detail-row span:first-child{min-width:100px;color:#64748b;font-weight:500}
  .desc{margin:.25rem 0;color:#1e293b}
  .suggestion{background:#eff6ff;border:1px solid #bfdbfe;border-radius:.5rem;padding:.75rem}
  .sug-title{font-weight:700;color:#1d4ed8;margin-bottom:.4rem}
  .sug-action{font-size:.8rem;color:#2563eb;margin-top:.4rem}
  .resolved-info{background:#f0fdf4;border:1px solid #86efac;border-radius:.5rem;padding:.75rem}
  .res-title{font-weight:700;color:#16a34a;margin-bottom:.4rem}
  .res-time{font-size:.78rem;color:#64748b;margin-top:.4rem}
  .resolve-form{display:flex;flex-direction:column;gap:.5rem;border-top:1px solid #e2e8f0;padding-top:.75rem;margin-top:.25rem}
  .form-lbl{font-size:.85rem;font-weight:600;color:#374151}
  .req{color:#dc2626}
  .textarea{width:100%;padding:.6rem .8rem;border:1px solid #e2e8f0;border-radius:.5rem;font-size:.85rem;resize:vertical;box-sizing:border-box}
  .btn-resolve{background:#10b981;color:#fff;border:none;padding:.65rem 1.2rem;border-radius:.5rem;cursor:pointer;font-weight:600;transition:background .2s}
  .btn-resolve:hover:not(:disabled){background:#059669}
  .btn-resolve:disabled{opacity:.6;cursor:not-allowed}
</style>
