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
    return s==='CRITICAL'?'bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 border-red-300 dark:border-red-800':s==='ERROR'?'bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 border-orange-300 dark:border-orange-800':s==='WARNING'?'bg-yellow-50 dark:bg-yellow-900/20 text-yellow-600 dark:text-yellow-400 border-yellow-300 dark:border-yellow-800':'bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 border-blue-300 dark:border-blue-800';
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
<div class="p-6 max-w-6xl mx-auto" dir="rtl">
  <div class="flex justify-between items-start mb-6">
    <div>
      <h1 class="text-2xl font-bold text-gray-800 dark:text-white">⚡ مركز التعارضات</h1>
      <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">كشف وتحليل وحل تعارضات المزامنة</p>
    </div>
    <button class="bg-blue-50 dark:bg-blue-900/200 hover:bg-blue-600 disabled:opacity-60 text-white px-5 py-2 rounded-lg transition-colors" on:click={load} disabled={loading}>⟳ تحديث</button>
  </div>

  {#if error}<div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 text-red-600 dark:text-red-400 px-4 py-3 rounded-lg mb-4">{error}</div>{/if}

  <!-- Summary -->
  {#if summary}
  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
    <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow text-center border-t-4 border-blue-500"><div class="text-2xl mb-2">📊</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.total}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">إجمالي التعارضات</div></div>
    <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow text-center border-t-4 {summary.unresolved>0?'border-red-500':'border-green-500'}"><div class="text-2xl mb-2">{summary.unresolved>0?'🔴':'✅'}</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.unresolved}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">غير محلول</div></div>
    <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow text-center border-t-4 border-green-500"><div class="text-2xl mb-2">✅</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.total-summary.unresolved}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">محلول</div></div>
    <div class="bg-white dark:bg-gray-800 rounded-xl p-5 shadow text-center border-t-4 border-amber-500"><div class="text-2xl mb-2">⚡</div><div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.recentConflicts.length}</div><div class="text-xs text-gray-500 dark:text-gray-400 mt-1">أحدث التعارضات</div></div>
  </div>
  {/if}

  <!-- Filters -->
  <div class="flex items-center gap-4 flex-wrap bg-white dark:bg-gray-800 rounded-lg p-3 px-4 shadow mb-4">
    <label class="flex items-center gap-2 text-sm cursor-pointer text-gray-800 dark:text-gray-200">
      <input type="checkbox" bind:checked={showUnresolvedOnly} on:change={load}/>
      عرض غير المحلولة فقط
    </label>
    <select bind:value={filterType} class="px-3 py-1.5 border border-gray-200 dark:border-gray-700 rounded-lg text-sm text-gray-800 dark:text-white bg-white dark:bg-gray-900">
      <option value="">كل الأنواع</option>
      <option value="STALE_IMPORT">استيراد قديم</option>
      <option value="DUPLICATE_PACKAGE">حزمة مكررة</option>
      <option value="OUT_OF_ORDER_REPORT">خارج الترتيب</option>
      <option value="DIVERGENT_STOCK_STATE">تباين المخزون</option>
      <option value="REPLAY_ATTEMPT">إعادة تشغيل</option>
    </select>
    <select bind:value={filterSeverity} class="px-3 py-1.5 border border-gray-200 dark:border-gray-700 rounded-lg text-sm text-gray-800 dark:text-white bg-white dark:bg-gray-900">
      <option value="">كل الشدد</option>
      <option value="CRITICAL">حرج</option>
      <option value="ERROR">خطأ</option>
      <option value="WARNING">تحذير</option>
      <option value="INFO">معلومة</option>
    </select>
    <span class="mr-auto text-sm text-gray-500 dark:text-gray-400">{filtered.length} نتيجة</span>
  </div>

  <!-- Conflicts Table -->
  {#if loading && !conflicts.length}
    <div class="text-center p-12 bg-white dark:bg-gray-800 rounded-xl shadow">جارٍ تحميل التعارضات...</div>
  {:else if filtered.length === 0}
    <div class="text-center p-8 bg-green-50 dark:bg-green-900/20 border border-green-300 dark:border-green-800 rounded-xl text-green-600 dark:text-green-400 font-semibold">✅ لا توجد تعارضات تطابق الفلاتر المحددة</div>
  {:else}
    <div class="bg-white dark:bg-gray-800 rounded-xl shadow overflow-hidden mb-6">
      <table class="w-full border-collapse">
        <thead><tr>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">الشدة</th>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">النوع</th>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">معرف الحزمة</th>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">العقدة المصدر</th>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">الوقت</th>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">الحالة</th>
          <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">إجراء</th>
        </tr></thead>
        <tbody>
          {#each filtered as c}
          <tr class="{c.resolved?'opacity-65':''}">
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle"><span class="px-2 py-1 rounded-md text-xs font-bold border {sevCls(c.severity)}">{sevIcon(c.severity)} {c.severity}</span></td>
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle"><span class="bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300 px-2 py-1 rounded text-xs">{c.conflictTypeDisplay}</span></td>
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle"><code class="font-mono text-xs text-gray-500 dark:text-gray-400" title={c.packageId}>{c.packageId.slice(0,12)}...</code></td>
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle"><code class="dark:text-gray-300">{c.sourceNodeId}</code></td>
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle text-xs text-gray-500 dark:text-gray-400">{fmt(c.createdAt)}</td>
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle">
              {#if c.resolved}
                <span class="text-green-600 dark:text-green-400 text-sm font-semibold">✅ محلول</span>
              {:else}
                <span class="text-amber-600 dark:text-amber-400 text-sm font-semibold">⏳ معلق</span>
              {/if}
            </td>
            <td class="px-4 py-2.5 text-sm border-b border-gray-100 dark:border-gray-700 align-middle">
              {#if !c.resolved}
                <button class="bg-blue-50 dark:bg-blue-900/200 hover:bg-blue-600 text-white px-3 py-1.5 rounded-md text-xs cursor-pointer" on:click={() => { selectedConflict = c; resolveNote = ''; }}>
                  حل
                </button>
              {:else}
                <button class="bg-gray-50 dark:bg-gray-9000 hover:bg-gray-600 text-white px-3 py-1.5 rounded-md text-xs cursor-pointer" on:click={() => selectedConflict = c}>
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
  class="fixed inset-0 bg-black/50 dark:bg-black/70 flex items-center justify-center z-50"
  role="dialog"
  aria-modal="true"
  aria-label="تفاصيل التعارض"
  tabindex="-1"
  on:click|self={() => selectedConflict = null}
  on:keydown={(e) => e.key === 'Escape' && (selectedConflict = null)}
>
  <div class="bg-white dark:bg-gray-800 rounded-2xl w-11/12 max-w-2xl max-h-[85vh] overflow-y-auto shadow-2xl" dir="rtl">
    <div class="flex justify-between items-center p-4 px-6 border-b border-gray-200 dark:border-gray-700">
      <h2 class="text-lg font-bold text-gray-800 dark:text-white m-0">تفاصيل التعارض</h2>
      <button class="bg-transparent border-none text-xl cursor-pointer text-gray-500 dark:text-gray-400 hover:text-gray-800 dark:text-gray-100 dark:hover:text-white" on:click={() => selectedConflict = null}>✕</button>
    </div>
    <div class="p-6 flex flex-col gap-3">
      <div class="flex items-baseline gap-2 text-sm"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">المعرف</span><code class="dark:text-gray-300">{selectedConflict.id}</code></div>
      <div class="flex items-baseline gap-2 text-sm"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">النوع</span><strong class="dark:text-white">{selectedConflict.conflictTypeDisplay}</strong></div>
      <div class="flex items-baseline gap-2 text-sm"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">الشدة</span><span class="px-2 py-1 rounded-md text-xs font-bold border {sevCls(selectedConflict.severity)}">{selectedConflict.severity}</span></div>
      <div class="flex items-baseline gap-2 text-sm"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">معرف الحزمة</span><code class="dark:text-gray-300">{selectedConflict.packageId}</code></div>
      <div class="flex items-baseline gap-2 text-sm"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">العقدة المصدر</span><code class="dark:text-gray-300">{selectedConflict.sourceNodeId}</code></div>
      <div class="flex items-baseline gap-2 text-sm"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">الوصف</span><p class="my-1 text-gray-800 dark:text-gray-200">{selectedConflict.description}</p></div>
      {#if selectedConflict.suggestedResolution}
        <div class="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-3 mt-2">
          <div class="font-bold text-blue-700 dark:text-blue-400 mb-1">💡 الاقتراح المقترح</div>
          <p class="text-sm dark:text-blue-100">{selectedConflict.suggestedResolution.description}</p>
          <div class="text-xs text-blue-600 dark:text-blue-300 mt-2">الإجراء: <strong>{selectedConflict.suggestedResolution.action}</strong></div>
        </div>
      {/if}
      {#if selectedConflict.resolved}
        <div class="bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg p-3 mt-2">
          <div class="font-bold text-green-600 dark:text-green-400 mb-1">✅ تم الحل بواسطة: {selectedConflict.resolvedBy}</div>
          <p class="text-sm dark:text-green-100">{selectedConflict.resolutionNote}</p>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-2">في {fmt(selectedConflict.resolvedAt)}</div>
        </div>
      {:else}
        <div class="flex flex-col gap-2 border-t border-gray-200 dark:border-gray-700 pt-3 mt-1">
          <label class="text-sm font-semibold text-gray-700 dark:text-gray-300" for="resolve-note">ملاحظة الحل <span class="text-red-600">*</span></label>
          <textarea id="resolve-note" bind:value={resolveNote} class="w-full p-2.5 border border-gray-200 dark:border-gray-700 rounded-lg text-sm resize-y box-border bg-white dark:bg-gray-900 text-gray-800 dark:text-gray-100" rows="3" placeholder="اذكر سبب الحل وما تم اتخاذه..."></textarea>
          <button class="bg-green-500 hover:bg-green-600 disabled:opacity-60 disabled:cursor-not-allowed text-white border-none py-2 px-5 rounded-lg cursor-pointer font-semibold transition-colors mt-2" on:click={doResolve} disabled={resolving || !resolveNote.trim()}>
            {resolving ? 'جارٍ الحفظ...' : '✅ تأكيد الحل'}
          </button>
        </div>
      {/if}
    </div>
  </div>
</div>
{/if}
</Layout>
