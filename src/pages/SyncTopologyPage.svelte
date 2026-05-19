<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getSyncHealth, getConflictSummary, getSettings } from '../lib/tauri';
  import type { SyncNodeHealth, ConflictSummary, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { formatErrorMessage } from '../lib/errors';
  import { createOperation } from '../lib/operationGuard';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';

  const syncOp = createOperation();
  const loading = syncOp.loading;
  const error = syncOp.error;

  let nodes: SyncNodeHealth[] = [];
  let summary: ConflictSummary | null = null;
  let observer: MutationObserver | null = null;
  let settings: Settings | null = null;

  async function load() {
    await syncOp.run(async () => {
      [nodes, summary] = await Promise.all([getSyncHealth(), getConflictSummary()]);
    });
  }

  function fmt(ts: string|null) { return ts ? new Date(ts).toLocaleString('ar-DZ') : '—'; }
  function stIcon(s: string) { return s==='HEALTHY'?'✅':s==='DEGRADED'?'⚠️':s==='CRITICAL'?'🔴':'❓'; }
  function stIntent(s: string): 'success'|'warning'|'danger'|'neutral' { return s==='HEALTHY'?'success':s==='DEGRADED'?'warning':s==='CRITICAL'?'danger':'neutral'; }

  // Simple canvas-based node graph
  let canvas: HTMLCanvasElement;
  $: if (canvas && nodes.length > 0) drawGraph();

  function drawGraph() {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H);

    // Get dark mode setting via current document class
    const isDark = document.documentElement.classList.contains('dark');
    const textColor = isDark ? '#f3f4f6' : '#fff';
    const hubColor = isDark ? '#334155' : '#1e293b';

    // Central hub
    const cx = W/2, cy = H/2;
    const r = Math.min(W, H) * 0.32;

    // Draw central node
    ctx.beginPath();
    ctx.arc(cx, cy, 28, 0, Math.PI*2);
    ctx.fillStyle = hubColor;
    ctx.fill();
    ctx.fillStyle = textColor;
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
    
    // Redraw graph when theme changes
    observer = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        if (mutation.attributeName === 'class' && canvas && nodes.length > 0) {
          drawGraph();
        }
      });
    });
    observer.observe(document.documentElement, { attributes: true });
  });

  onDestroy(() => {
    observer?.disconnect();
  });

  $: nodeType = settings?.node_type || null;
</script>

<Layout {nodeType} title="طوبولوجيا المزامنة" subtitle="مراقبة العقد والعلاقات">
  <div dir="rtl">
    <AppPageHeader title="طوبولوجيا المزامنة" subtitle="خريطة العقد والحزم والصحة التشغيلية">
      <svelte:fragment slot="actions">
        <AppButton variant="primary" on:click={load} disabled={$loading}>⟳ تحديث</AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger">{$error}</AppAlert>
      </div>
    {/if}

    {#if $loading && !summary}
      <AppLoadingState message="جارٍ تحميل بيانات الشبكة..." />
    {:else}
      <!-- Summary Cards -->
      {#if summary}
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <AppCard class="border-t-4 border-blue-500" padding="sm">
          <div class="text-center">
            <div class="text-2xl mb-2">📦</div>
            <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.total}</div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">إجمالي التعارضات</div>
          </div>
        </AppCard>
        <AppCard class="border-t-4 border-red-500" padding="sm">
          <div class="text-center">
            <div class="text-2xl mb-2">🔴</div>
            <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.unresolved}</div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">غير محلول</div>
          </div>
        </AppCard>
        <AppCard class="border-t-4 border-green-500" padding="sm">
          <div class="text-center">
            <div class="text-2xl mb-2">🌐</div>
            <div class="text-2xl font-bold text-gray-800 dark:text-white">{nodes.length}</div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">عقد نشطة</div>
          </div>
        </AppCard>
        <AppCard class="border-t-4 border-purple-500" padding="sm">
          <div class="text-center">
            <div class="text-2xl mb-2">✅</div>
            <div class="text-2xl font-bold text-gray-800 dark:text-white">{summary.total - summary.unresolved}</div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">تعارضات محلولة</div>
          </div>
        </AppCard>
      </div>
      {/if}

      <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
        <!-- Node Graph -->
        <AppCard class="border-t-4 border-gray-200 dark:border-gray-700">
          <h2 class="font-semibold text-gray-800 dark:text-white mb-3">🗺️ خريطة العقد</h2>
          {#if nodes.length > 0}
            <canvas bind:this={canvas} width="400" height="320" class="w-full max-w-[400px] block mx-auto"></canvas>
            <p class="text-center text-xs text-gray-400 dark:text-gray-500 mt-2">أخضر = سليم &nbsp;|&nbsp; أصفر = متدهور &nbsp;|&nbsp; أحمر = حرج</p>
          {:else}
            <div class="text-center p-8 text-gray-400 dark:text-gray-500 italic">لا توجد بيانات عقد بعد. ستظهر هنا عند أول مزامنة.</div>
          {/if}
        </AppCard>

        <!-- Type Breakdown -->
        <div class="space-y-4">
          {#if summary && summary.byType.length > 0}
          <AppCard class="border-t-4 border-gray-200 dark:border-gray-700">
            <h2 class="font-semibold text-gray-800 dark:text-white mb-3">📊 التعارضات حسب النوع</h2>
            {#each summary.byType as t}
              <div class="flex items-center gap-2 mb-1.5 text-sm">
                <span class="min-w-[160px] text-gray-500 dark:text-gray-400">{t.conflictTypeDisplay}</span>
                <div class="flex-1 h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                  <div class="h-full bg-gradient-to-r from-blue-500 to-purple-500 rounded-full transition-all duration-500" style="width:{Math.min((t.count/summary.total)*100,100)}%"></div>
                </div>
                <span class="min-w-[30px] text-left font-semibold text-gray-800 dark:text-white">{t.count}</span>
              </div>
            {/each}
          </AppCard>
          {/if}

          {#if summary && summary.bySeverity.length > 0}
          <AppCard class="border-t-4 border-gray-200 dark:border-gray-700">
            <h2 class="font-semibold text-gray-800 dark:text-white mb-3">⚠️ التعارضات حسب الشدة</h2>
            {#each summary.bySeverity as s}
              <div class="flex items-center gap-2 mb-1.5 text-sm">
                <span class="min-w-[160px] text-gray-500 dark:text-gray-400">{s.severity}</span>
                <div class="flex-1 h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                  <div class="h-full rounded-full transition-all duration-500 {s.severity==='CRITICAL'?'bg-red-500':s.severity==='ERROR'?'bg-orange-500':s.severity==='WARNING'?'bg-yellow-500':'bg-blue-50 dark:bg-blue-900/20'}" style="width:{Math.min((s.count/summary.total)*100,100)}%"></div>
                </div>
                <span class="min-w-[30px] text-left font-semibold text-gray-800 dark:text-white">{s.count}</span>
              </div>
            {/each}
          </AppCard>
          {/if}
        </div>
      </div>

      <!-- Node Health Table -->
      {#if nodes.length > 0}
      <h2 class="font-semibold text-gray-800 dark:text-white mb-3">📋 تفاصيل العقد ({nodes.length})</h2>
      <AppCard padding="none" class="mb-6">
        <div class="overflow-x-auto">
          <table class="w-full border-collapse">
            <thead><tr>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">الحالة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">معرف العقدة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">آخر مزامنة</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">مستلم</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">مرفوض</th>
              <th class="bg-gray-50 dark:bg-gray-900 px-4 py-3 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700">إعادة تشغيل</th>
            </tr></thead>
            <tbody>
              {#each nodes as n}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 font-medium">
                  <AppBadge intent={stIntent(n.status)} size="sm">{stIcon(n.status)} {n.status}</AppBadge>
                </td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700"><code class="dark:text-gray-300">{n.nodeId}</code></td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 text-gray-800 dark:text-gray-300">{fmt(n.lastSyncTimestamp)}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 text-gray-800 dark:text-gray-300">{n.packagesReceived}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 {n.packagesRejected>0?'text-red-600 dark:text-red-400':'text-gray-800 dark:text-gray-300'}">{n.packagesRejected}</td>
                <td class="px-4 py-3 text-sm border-b border-gray-100 dark:border-gray-700 {n.replayAttempts>0?'text-orange-600 dark:text-orange-400':'text-gray-800 dark:text-gray-300'}">{n.replayAttempts}</td>
              </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </AppCard>
      {/if}
    {/if}
  </div>
</Layout>
