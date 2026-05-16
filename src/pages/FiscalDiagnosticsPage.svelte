<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { getSettings } from '../lib/tauri';
  import Layout from '../components/Layout.svelte';

  // ─── State (manual refresh only — no live updates, no websocket) ────────────
  let loading = false;
  let error: string | null = null;
  let selectedYear: number | null = new Date().getFullYear();
  let lastRefreshedAt: string | null = null;
  let nodeType: 'WILAYA' | 'UNIT' | null = null;

  // Existing data
  let inventory: any = null;

  // New data
  let bundle: {
    anomalyReport: {
      generatedAt: string;
      findings: Array<{
        severity: 'INFO' | 'WARNING' | 'CRITICAL';
        category: string;
        code: string;
        message: string;
        recommendation: string;
        context: any;
      }>;
    };
    recommendations: Array<{
      priority: 'LOW' | 'MEDIUM' | 'HIGH';
      code: string;
      title: string;
      message: string;
    }>;
    recentSnapshots: Array<{
      id: number;
      snapshotDate: string;
      fiscalYear: number;
      totalInventoryValue: number;
      productCount: number;
      movementCount: number;
      reportCount: number;
      integrityState: string;
      createdBy: string;
      createdAt: string;
    }>;
    timeline: Array<{
      timestamp: string;
      kind: string;
      fiscalYear: number | null;
      actor: string | null;
      summary: string;
      source: string;
    }>;
    fiscalIntegrity: {
      ok: boolean;
      warnings: Array<{ code: string; details: string }>;
    };
  } | null = null;

  // System health (kept from existing page) — fetched in parallel
  let systemHealth: any = null;

  async function refresh() {
    loading = true;
    error = null;
    try {
      const [b, inv, hp] = await Promise.all([
        invoke('get_advanced_diagnostics_bundle', { fiscalYear: selectedYear }),
        selectedYear !== null
          ? invoke('verify_inventory_integrity', { year: selectedYear })
          : Promise.resolve(null),
        invoke('get_system_health').catch(() => null),
      ]);
      bundle = b as any;
      inventory = inv;
      systemHealth = hp;
      lastRefreshedAt = new Date().toLocaleString();
    } catch (e: any) {
      error = typeof e === 'string' ? e : JSON.stringify(e);
    } finally {
      loading = false;
    }
  }

  async function takeSnapshot() {
    if (selectedYear === null) return;
    loading = true;
    error = null;
    try {
      await invoke('create_fiscal_operational_snapshot', { fiscalYear: selectedYear });
      await refresh();
    } catch (e: any) {
      error = typeof e === 'string' ? e : JSON.stringify(e);
    } finally {
      loading = false;
    }
  }

  function severityClass(sev: string): string {
    switch (sev) {
      case 'CRITICAL':
        return 'bg-red-50 border-red-300 text-red-900';
      case 'WARNING':
        return 'bg-yellow-50 border-yellow-300 text-yellow-900';
      default:
        return 'bg-blue-50 border-blue-300 text-blue-900';
    }
  }

  function priorityClass(p: string): string {
    switch (p) {
      case 'HIGH':
        return 'bg-red-100 text-red-800';
      case 'MEDIUM':
        return 'bg-yellow-100 text-yellow-800';
      default:
        return 'bg-gray-100 text-gray-800';
    }
  }

  function integrityClass(state: string): string {
    switch (state) {
      case 'OK':
        return 'text-green-700';
      case 'WARNINGS':
        return 'text-yellow-700';
      case 'CRITICAL':
        return 'text-red-700';
      default:
        return 'text-gray-600';
    }
  }

  function kindLabel(kind: string): string {
    return kind.replace(/_/g, ' ');
  }

  function formatTs(ts: string): string {
    try {
      return new Date(ts).toLocaleString();
    } catch {
      return ts;
    }
  }

  function bytesToMb(bytes: number): string {
    return (bytes / 1024 / 1024).toFixed(1);
  }

  function translateSeverity(sev: string): string {
    switch (sev) {
      case 'CRITICAL': return 'حرجة';
      case 'WARNING': return 'تحذير';
      case 'INFO': return 'معلومات';
      default: return sev;
    }
  }

  function translatePriority(p: string): string {
    switch (p) {
      case 'HIGH': return 'عالية';
      case 'MEDIUM': return 'متوسطة';
      case 'LOW': return 'منخفضة';
      default: return p;
    }
  }

  function translateStatus(s: string): string {
    switch (s) {
      case 'HEALTHY': return 'سليم';
      case 'WARNINGS': return 'تحذيرات';
      case 'CRITICAL': return 'حرج';
      case 'OK': return 'سليم';
      default: return s;
    }
  }

  function translateCode(code: string): string {
    switch (code) {
      case 'ANOMALY_A_OUTBOUND_SPIKE': return 'طفرة في حركة الخروج';
      case 'ANOMALY_B_INVENTORY_JUMP': return 'قفزة في قيمة المخزون';
      case 'ANOMALY_C_LOW_REPORTING': return 'نشاط تقارير منخفض';
      case 'ANOMALY_D_FAILED_IMPORTS': return 'فشل في الاستيراد';
      case 'ANOMALY_E_INTEGRITY_FAILURES': return 'فشل في التحقق من السلامة';
      case 'REC_DB_GROWTH_HIGH': return 'نمو كبير لقاعدة البيانات';
      case 'REC_DB_GROWTH': return 'نمو قاعدة البيانات';
      case 'REC_FAILED_IMPORTS': return 'عمليات استيراد فاشلة';
      case 'REC_INTEGRITY_FAILURES': return 'فشل في السلامة';
      case 'REC_NO_BACKUP': return 'لا توجد نسخة احتياطية';
      case 'REC_BACKUP_VERY_STALE': return 'نسخة احتياطية قديمة جداً';
      case 'REC_BACKUP_STALE': return 'نسخة احتياطية قديمة';
      case 'REC_CONFLICTS_HIGH': return 'تعارضات مزامنة كثيرة';
      case 'REC_CONFLICTS': return 'تعارضات مزامنة';
      default: return code;
    }
  }

  // Initial load — manual only afterwards.
  onMount(async () => {
    try {
      const settings = await getSettings();
      nodeType = settings.node_type;
    } catch {}
    refresh();
  });
</script>

<Layout {nodeType} title="تشخيصات النظام المتقدمة" subtitle="رؤية تشغيلية موجَّهة">
  <div class="p-6 space-y-6 max-w-7xl mx-auto" dir="rtl">
  <!-- ─── Header ──────────────────────────────────────────────────────────── -->
  <div class="flex items-center justify-between border-b pb-4">
    <div>
      <h1 class="text-2xl font-bold">تشخيصات النظام المتقدمة</h1>
      <p class="text-sm text-gray-600 mt-1">
        رؤية تشغيلية موجَّهة — يدوية بالكامل، لا تحديثات تلقائية.
      </p>
    </div>
    <div class="flex items-center gap-2">
      <label class="text-sm">
        السنة:
        <input
          type="number"
          bind:value={selectedYear}
          class="border rounded px-2 py-1 w-24 mx-2"
          min="2020"
          max="2100"
        />
      </label>
      <button
        on:click={refresh}
        disabled={loading}
        class="bg-blue-600 hover:bg-blue-700 disabled:bg-gray-400 text-white px-4 py-2 rounded"
      >
        {loading ? '...جاري التحديث' : 'تحديث يدوي'}
      </button>
      <button
        on:click={takeSnapshot}
        disabled={loading || selectedYear === null}
        class="bg-green-600 hover:bg-green-700 disabled:bg-gray-400 text-white px-4 py-2 rounded"
        title="إنشاء لقطة تشغيلية يدوية"
      >
        لقطة جديدة
      </button>
    </div>
  </div>

  {#if lastRefreshedAt}
    <div class="text-xs text-gray-500">آخر تحديث: {lastRefreshedAt}</div>
  {/if}

  {#if error}
    <div class="border border-red-300 bg-red-50 text-red-800 p-3 rounded">
      <div class="font-semibold">خطأ:</div>
      <div class="text-sm">{error}</div>
    </div>
  {/if}

  <!-- ─── Anomaly Findings ────────────────────────────────────────────────── -->
  {#if bundle}
    <section>
      <h2 class="text-xl font-semibold mb-3">
        🔍 الاكتشافات التشغيلية
        <span class="text-sm font-normal text-gray-500">
          ({bundle.anomalyReport.findings.length})
        </span>
      </h2>
      {#if bundle.anomalyReport.findings.length === 0}
        <div class="text-sm text-green-700 bg-green-50 border border-green-200 rounded p-3">
          لم يتم رصد أي شذوذ تشغيلي.
        </div>
      {:else}
        <div class="space-y-2">
          {#each bundle.anomalyReport.findings as f}
            <div class="border-r-4 rounded p-3 {severityClass(f.severity)}">
              <div class="flex items-center justify-between mb-1">
                <span class="font-mono text-xs">{translateCode(f.code)}</span>
                <span class="text-xs font-semibold">{translateSeverity(f.severity)}</span>
              </div>
              <div class="text-sm font-medium">{f.message}</div>
              <div class="text-xs mt-1 opacity-80">
                <strong>توصية:</strong>
                {f.recommendation}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- ─── Recommendations ───────────────────────────────────────────────── -->
    <section>
      <h2 class="text-xl font-semibold mb-3">
        💡 توصيات تشغيلية
        <span class="text-sm font-normal text-gray-500">
          ({bundle.recommendations.length})
        </span>
      </h2>
      {#if bundle.recommendations.length === 0}
        <div class="text-sm text-gray-600">لا توجد توصيات نشطة حالياً.</div>
      {:else}
        <div class="space-y-2">
          {#each bundle.recommendations as r}
            <div class="border rounded p-3 bg-white">
              <div class="flex items-center justify-between">
                <div class="font-medium">{r.title}</div>
                <span class="text-xs px-2 py-1 rounded {priorityClass(r.priority)}">
                  {translatePriority(r.priority)}
                </span>
              </div>
              <div class="text-sm text-gray-700 mt-1">{r.message}</div>
              <div class="font-mono text-[10px] text-gray-400 mt-1">{translateCode(r.code)}</div>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- ─── Fiscal Integrity Summary ──────────────────────────────────────── -->
    <section>
      <h2 class="text-xl font-semibold mb-3">⚖️ سلامة المالية</h2>
      <div class="border rounded p-3 bg-white">
        <div class="font-medium">
          الحالة:
          <span class={bundle.fiscalIntegrity.ok ? 'text-green-700' : 'text-yellow-700'}>
            {bundle.fiscalIntegrity.ok ? 'سليمة' : 'تحذيرات'}
          </span>
        </div>
        {#if bundle.fiscalIntegrity.warnings.length > 0}
          <ul class="text-sm mt-2 list-disc pr-5 space-y-1">
            {#each bundle.fiscalIntegrity.warnings as w}
              <li><span class="font-mono text-xs">{w.code}</span> — {w.details}</li>
            {/each}
          </ul>
        {/if}
      </div>
    </section>

    <!-- ─── Inventory Mismatches (existing capability) ────────────────────── -->
    {#if inventory}
      <section>
        <h2 class="text-xl font-semibold mb-3">
          📦 تطابق المخزون
          <span class="text-sm font-normal text-gray-500">
            (فحص {inventory.checked_products} منتج، {inventory.mismatch_count} اختلاف)
          </span>
        </h2>
        {#if inventory.mismatch_count === 0}
          <div class="text-sm text-green-700 bg-green-50 border border-green-200 rounded p-3">
            لا توجد اختلافات في المخزون.
          </div>
        {:else}
          <div class="overflow-auto border rounded">
            <table class="min-w-full text-sm">
              <thead class="bg-gray-100">
                <tr>
                  <th class="px-3 py-2 text-right">المنتج</th>
                  <th class="px-3 py-2 text-right">المتوقع</th>
                  <th class="px-3 py-2 text-right">الفعلي</th>
                  <th class="px-3 py-2 text-right">الفرق</th>
                </tr>
              </thead>
              <tbody>
                {#each inventory.issues as it}
                  <tr class="border-t">
                    <td class="px-3 py-1 font-mono text-xs">{it.product_id}</td>
                    <td class="px-3 py-1">{it.expected_quantity}</td>
                    <td class="px-3 py-1">{it.actual_quantity}</td>
                    <td class="px-3 py-1 text-red-700">{it.delta.toFixed(2)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    {/if}

    <!-- ─── Operational Snapshots / Growth Analysis ───────────────────────── -->
    <section>
      <h2 class="text-xl font-semibold mb-3">
        📈 اللقطات التشغيلية
        <span class="text-sm font-normal text-gray-500">
          ({bundle.recentSnapshots.length})
        </span>
      </h2>
      {#if bundle.recentSnapshots.length === 0}
        <div class="text-sm text-gray-600">
          لا توجد لقطات بعد. اضغط «لقطة جديدة» لإنشاء الأولى.
        </div>
      {:else}
        <div class="overflow-auto border rounded">
          <table class="min-w-full text-sm">
            <thead class="bg-gray-100">
              <tr>
                <th class="px-3 py-2 text-right">التاريخ</th>
                <th class="px-3 py-2 text-right">السنة</th>
                <th class="px-3 py-2 text-right">قيمة المخزون</th>
                <th class="px-3 py-2 text-right">المنتجات</th>
                <th class="px-3 py-2 text-right">الحركات</th>
                <th class="px-3 py-2 text-right">التقارير</th>
                <th class="px-3 py-2 text-right">السلامة</th>
                <th class="px-3 py-2 text-right">بواسطة</th>
              </tr>
            </thead>
            <tbody>
              {#each bundle.recentSnapshots as s}
                <tr class="border-t">
                  <td class="px-3 py-1">{s.snapshotDate}</td>
                  <td class="px-3 py-1">{s.fiscalYear}</td>
                  <td class="px-3 py-1">{s.totalInventoryValue.toFixed(2)}</td>
                  <td class="px-3 py-1">{s.productCount}</td>
                  <td class="px-3 py-1">{s.movementCount}</td>
                  <td class="px-3 py-1">{s.reportCount}</td>
                  <td class="px-3 py-1 font-semibold {integrityClass(s.integrityState)}">
                    {s.integrityState}
                  </td>
                  <td class="px-3 py-1 text-xs">{s.createdBy}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <!-- ─── Fiscal Timeline ───────────────────────────────────────────────── -->
    <section>
      <h2 class="text-xl font-semibold mb-3">
        🕒 الخط الزمني الموحَّد
        <span class="text-sm font-normal text-gray-500">
          ({bundle.timeline.length})
        </span>
      </h2>
      {#if bundle.timeline.length === 0}
        <div class="text-sm text-gray-600">لا توجد أحداث مسجّلة في النطاق المحدّد.</div>
      {:else}
        <div class="border rounded">
          <ul class="divide-y">
            {#each bundle.timeline as ev}
              <li class="p-3 hover:bg-gray-50">
                <div class="flex items-center justify-between">
                  <span class="font-mono text-xs text-gray-500">{formatTs(ev.timestamp)}</span>
                  <span class="text-xs px-2 py-0.5 rounded bg-gray-100">
                    {kindLabel(ev.kind)}
                  </span>
                </div>
                <div class="text-sm mt-1">{ev.summary}</div>
                <div class="text-xs text-gray-500 mt-1">
                  {#if ev.actor}بواسطة {ev.actor} — {/if}
                  مصدر: {ev.source}
                  {#if ev.fiscalYear !== null} — سنة {ev.fiscalYear}{/if}
                </div>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </section>

    <!-- ─── Backup Health Summary (from existing system health) ───────────── -->
    {#if systemHealth}
      <section>
        <h2 class="text-xl font-semibold mb-3">💾 صحة النسخ الاحتياطية والقاعدة</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div class="border rounded p-3 bg-white">
            <div class="text-xs uppercase text-gray-500">قاعدة البيانات</div>
            <div class="font-medium mt-1">
              {translateStatus(systemHealth.databaseStatus.status)} — {systemHealth.databaseStatus.message}
            </div>
            <div class="text-xs text-gray-500 mt-1">
              الحجم: {bytesToMb(systemHealth.storageUsageBytes)} MB
            </div>
          </div>
          <div class="border rounded p-3 bg-white">
            <div class="text-xs uppercase text-gray-500">النسخ الاحتياطية</div>
            <div class="font-medium mt-1">
              {translateStatus(systemHealth.backupStatus.status)} — {systemHealth.backupStatus.message}
            </div>
            {#if systemHealth.backupStatus.lastBackup}
              <div class="text-xs text-gray-500 mt-1">
                آخر نسخة: {formatTs(systemHealth.backupStatus.lastBackup)}
              </div>
            {/if}
          </div>
        </div>
      </section>
    {/if}
  {/if}
</div>
</Layout>
