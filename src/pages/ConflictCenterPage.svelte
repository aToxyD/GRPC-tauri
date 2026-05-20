<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listSyncConflicts, resolveSyncConflict, getConflictSummary, getSettings } from '../lib/tauri';
  import type { SyncConflict, ConflictSummary, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { formatErrorMessage } from '../lib/errors';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppDialog from '../lib/components/ui/AppDialog.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppTextarea from '../lib/components/ui/AppTextarea.svelte';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const conflictsOp = createOperation({ scope });
  const loading = conflictsOp.loading;
  const error = conflictsOp.error;

  const { loading: resolving, guard } = createOperationGuard({ scope });

  let conflicts: SyncConflict[] = [];
  let summary: ConflictSummary | null = null;
  let settings: Settings | null = null;
  let showUnresolvedOnly = false;
  let filterType = '';
  let filterSeverity = '';
  let selectedConflict: SyncConflict | null = null;
  let resolveNote = '';

  async function load() {
    await conflictsOp.run(async () => {
      [conflicts, summary] = await Promise.all([
        listSyncConflicts(showUnresolvedOnly),
        getConflictSummary()
      ]);
    });
  }

  async function doResolve() {
    if (!selectedConflict || !resolveNote.trim()) return;
    const conflict = selectedConflict;
    await guard(async () => {
      try {
        await resolveSyncConflict(conflict.id, resolveNote);
        selectedConflict = null;
        resolveNote = '';
        await load();
      } catch(e) {
        conflictsOp.error.set(formatErrorMessage(e));
      }
    });
  }

  function fmt(ts: string | null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }

  function sevIntent(s: string): 'danger' | 'warning' | 'info' {
    if (s === 'CRITICAL' || s === 'ERROR') return 'danger';
    if (s === 'WARNING') return 'warning';
    return 'info';
  }

  function sevIcon(s: string) {
    return s === 'CRITICAL' ? '🔴' : s === 'ERROR' ? '🟠' : s === 'WARNING' ? '🟡' : '🔵';
  }

  $: filtered = conflicts.filter(c => {
    if (filterType && c.conflictType !== filterType) return false;
    if (filterSeverity && c.severity !== filterSeverity) return false;
    return true;
  });

  onMount(async () => {
    try { settings = await getSettings(); } catch (_e) { /* optional settings */ }
    await load();
  });

  $: nodeType = settings?.node_type || null;
</script>

<Layout {nodeType} title="مركز التعارضات" subtitle="كشف وإدارة تعارضات المزامنة">
  <div class="p-6 max-w-6xl mx-auto" dir="rtl">

    <!-- رأس الصفحة -->
    <AppPageHeader title="⚡ مركز التعارضات" subtitle="كشف وتحليل وحل تعارضات المزامنة">
      <svelte:fragment slot="actions">
        <AppButton variant="secondary" size="sm" loading={$loading} on:click={load}>⟳ تحديث</AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => conflictsOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    <!-- الملخص -->
    {#if summary}
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <AppCard padding="sm" class="text-center border-t-4 border-blue-500">
          <div class="text-2xl mb-2">📊</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.total}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">إجمالي التعارضات</div>
        </AppCard>
        <AppCard padding="sm" class="text-center border-t-4 {summary.unresolved > 0 ? 'border-red-500' : 'border-green-500'}">
          <div class="text-2xl mb-2">{summary.unresolved > 0 ? '🔴' : '✅'}</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.unresolved}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">غير محلول</div>
        </AppCard>
        <AppCard padding="sm" class="text-center border-t-4 border-green-500">
          <div class="text-2xl mb-2">✅</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.total - summary.unresolved}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">محلول</div>
        </AppCard>
        <AppCard padding="sm" class="text-center border-t-4 border-amber-500">
          <div class="text-2xl mb-2">⚡</div>
          <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.recentConflicts.length}</div>
          <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">أحدث التعارضات</div>
        </AppCard>
      </div>
    {/if}

    <!-- الفلاتر -->
    <AppCard padding="sm" class="mb-4">
      <div class="flex items-center gap-4 flex-wrap">
        <label class="flex items-center gap-2 text-sm cursor-pointer text-gray-800 dark:text-gray-200">
          <input type="checkbox" bind:checked={showUnresolvedOnly} on:change={load} class="accent-civil-blue"/>
          عرض غير المحلولة فقط
        </label>
        <select bind:value={filterType} class="px-3 py-1.5 border border-gray-200 dark:border-gray-700 rounded-lg text-sm text-gray-800 dark:text-white bg-white dark:bg-gray-700">
          <option value="">كل الأنواع</option>
          <option value="STALE_IMPORT">استيراد قديم</option>
          <option value="DUPLICATE_PACKAGE">حزمة مكررة</option>
          <option value="OUT_OF_ORDER_REPORT">خارج الترتيب</option>
          <option value="DIVERGENT_STOCK_STATE">تباين المخزون</option>
          <option value="REPLAY_ATTEMPT">إعادة تشغيل</option>
        </select>
        <select bind:value={filterSeverity} class="px-3 py-1.5 border border-gray-200 dark:border-gray-700 rounded-lg text-sm text-gray-800 dark:text-white bg-white dark:bg-gray-700">
          <option value="">كل الشدد</option>
          <option value="CRITICAL">حرج</option>
          <option value="ERROR">خطأ</option>
          <option value="WARNING">تحذير</option>
          <option value="INFO">معلومة</option>
        </select>
        <span class="mr-auto text-sm text-gray-500 dark:text-gray-400">{filtered.length} نتيجة</span>
      </div>
    </AppCard>

    <!-- جدول التعارضات -->
    {#if $loading && !conflicts.length}
      <AppLoadingState message="جارٍ تحميل التعارضات..." />
    {:else}
      <AppTable empty={filtered.length === 0} emptyMessage="لا توجد تعارضات تطابق الفلاتر المحددة" caption="جدول تعارضات المزامنة">
        <svelte:fragment slot="head">
          <th class="table-header">الشدة</th>
          <th class="table-header">النوع</th>
          <th class="table-header">معرف الحزمة</th>
          <th class="table-header">العقدة المصدر</th>
          <th class="table-header">الوقت</th>
          <th class="table-header">الحالة</th>
          <th class="table-header">إجراء</th>
        </svelte:fragment>

        {#each filtered as c}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/40 transition-colors {c.resolved ? 'opacity-60' : ''}">
            <td class="table-cell">
              <AppBadge intent={sevIntent(c.severity)}>{sevIcon(c.severity)} {c.severity}</AppBadge>
            </td>
            <td class="table-cell">
              <span class="bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300 px-2 py-1 rounded text-xs">{c.conflictTypeDisplay}</span>
            </td>
            <td class="table-cell">
              <code class="font-mono text-xs text-gray-500 dark:text-gray-400" title={c.packageId}>{c.packageId.slice(0,12)}...</code>
            </td>
            <td class="table-cell">
              <code class="text-xs dark:text-gray-300">{c.sourceNodeId}</code>
            </td>
            <td class="table-cell text-xs text-gray-500 dark:text-gray-400">{fmt(c.createdAt)}</td>
            <td class="table-cell">
              {#if c.resolved}
                <AppBadge intent="success">✅ محلول</AppBadge>
              {:else}
                <AppBadge intent="warning">⏳ معلق</AppBadge>
              {/if}
            </td>
            <td class="table-cell">
              {#if !c.resolved}
                <AppButton variant="primary" size="sm" on:click={() => { selectedConflict = c; resolveNote = ''; }}>حل</AppButton>
              {:else}
                <AppButton variant="ghost" size="sm" on:click={() => selectedConflict = c}>عرض</AppButton>
              {/if}
            </td>
          </tr>
        {/each}
      </AppTable>
    {/if}
  </div>

  <!-- حوار تفاصيل التعارض / الحل -->
  <AppDialog
    open={selectedConflict !== null}
    title="تفاصيل التعارض"
    size="lg"
    on:close={() => selectedConflict = null}
  >
    {#if selectedConflict}
      <div class="flex flex-col gap-3 text-sm">
        <div class="flex items-baseline gap-2"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">المعرف</span><code class="dark:text-gray-300 break-all">{selectedConflict.id}</code></div>
        <div class="flex items-baseline gap-2"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">النوع</span><strong class="dark:text-white">{selectedConflict.conflictTypeDisplay}</strong></div>
        <div class="flex items-baseline gap-2"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">الشدة</span><AppBadge intent={sevIntent(selectedConflict.severity)}>{selectedConflict.severity}</AppBadge></div>
        <div class="flex items-baseline gap-2"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">معرف الحزمة</span><code class="dark:text-gray-300 break-all">{selectedConflict.packageId}</code></div>
        <div class="flex items-baseline gap-2"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">العقدة المصدر</span><code class="dark:text-gray-300">{selectedConflict.sourceNodeId}</code></div>
        <div class="flex items-baseline gap-2"><span class="min-w-[100px] text-gray-500 dark:text-gray-400 font-medium">الوصف</span><p class="my-0 text-gray-800 dark:text-gray-200">{selectedConflict.description}</p></div>

        {#if selectedConflict.suggestedResolution}
          <AppAlert intent="info" title="💡 الاقتراح المقترح">
            <p class="text-sm">{selectedConflict.suggestedResolution.description}</p>
            <p class="text-xs mt-1">الإجراء: <strong>{selectedConflict.suggestedResolution.action}</strong></p>
          </AppAlert>
        {/if}

        {#if selectedConflict.resolved}
          <AppAlert intent="success" title="✅ تم الحل بواسطة: {selectedConflict.resolvedBy}">
            <p class="text-sm">{selectedConflict.resolutionNote}</p>
            <p class="text-xs mt-1">في {fmt(selectedConflict.resolvedAt)}</p>
          </AppAlert>
        {:else}
          <div class="border-t border-gray-200 dark:border-gray-700 pt-3 mt-1">
            <AppTextarea
              id="resolve-note"
              label="ملاحظة الحل"
              bind:value={resolveNote}
              rows={3}
              required
              placeholder="اذكر سبب الحل وما تم اتخاذه..."
            />
          </div>
        {/if}
      </div>
    {/if}

    <svelte:fragment slot="actions">
      <AppButton variant="secondary" on:click={() => selectedConflict = null}>إغلاق</AppButton>
      {#if selectedConflict && !selectedConflict.resolved}
        <AppButton
          variant="primary"
          loading={$resolving}
          disabled={!resolveNote.trim()}
          on:click={doResolve}
        >
          ✅ تأكيد الحل
        </AppButton>
      {/if}
    </svelte:fragment>
  </AppDialog>
</Layout>
