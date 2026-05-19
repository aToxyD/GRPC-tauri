<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getSettings,
    getAdvancedDiagnosticsBundle,
    verifyInventoryIntegrity,
    getSystemHealth,
    createFiscalOperationalSnapshot
  } from '../lib/tauri';
  import { createOperation } from '../lib/operationGuard';
  import Layout from '../components/Layout.svelte';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppSection from '../lib/components/ui/AppSection.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';

  import type { SystemHealthReport } from '../lib/types';

  interface InventoryIntegrityReport {
    checked_products: number;
    mismatch_count: number;
    issues: Array<{
      product_id: string;
      expected_quantity: number;
      actual_quantity: number;
      delta: number;
    }>;
  }

  // ─── State (manual refresh only — no live updates, no websocket) ────────────
  const op = createOperation();
  const loading = op.loading;
  const error = op.error;
  let selectedYear: number | null = new Date().getFullYear();
  let lastRefreshedAt: string | null = null;
  let nodeType: 'WILAYA' | 'UNIT' | null = null;

  // Existing data
  let inventory: InventoryIntegrityReport | null = null;

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
        context: unknown;
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
  let systemHealth: SystemHealthReport | null = null;

  async function refresh() {
    await op.run(async () => {
      const [b, inv, hp] = await Promise.all([
        getAdvancedDiagnosticsBundle(selectedYear),
        selectedYear !== null
          ? verifyInventoryIntegrity(selectedYear)
          : Promise.resolve(null),
        getSystemHealth().catch(() => null),
      ]);
      bundle = b as typeof bundle;
      inventory = inv as InventoryIntegrityReport | null;
      systemHealth = hp as SystemHealthReport | null;
      lastRefreshedAt = new Date().toLocaleString('ar-DZ');
    });
  }

  async function takeSnapshot() {
    if (selectedYear === null) return;
    const year = selectedYear;
    await op.run(async () => {
      await createFiscalOperationalSnapshot(year);
      await refresh();
    });
  }

  function severityIntent(sev: string): 'danger' | 'warning' | 'info' {
    switch (sev) {
      case 'CRITICAL': return 'danger';
      case 'WARNING': return 'warning';
      default: return 'info';
    }
  }

  function priorityIntent(p: string): 'danger' | 'warning' | 'neutral' {
    switch (p) {
      case 'HIGH': return 'danger';
      case 'MEDIUM': return 'warning';
      default: return 'neutral';
    }
  }

  function integrityIntent(state: string): 'success' | 'warning' | 'danger' | 'neutral' {
    switch (state) {
      case 'OK': return 'success';
      case 'WARNINGS': return 'warning';
      case 'CRITICAL': return 'danger';
      default: return 'neutral';
    }
  }

  function kindLabel(kind: string): string {
    return kind.replace(/_/g, ' ');
  }

  function formatTs(ts: string): string {
    try {
      return new Date(ts).toLocaleString('ar-DZ');
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
      nodeType = settings.node_type as 'WILAYA' | 'UNIT' | null;
    } catch {}
    refresh();
  });
</script>

<Layout {nodeType} title="تشخيصات النظام المتقدمة" subtitle="رؤية تشغيلية موجَّهة">
  <div class="p-6 space-y-6 max-w-7xl mx-auto text-gray-900 dark:text-white" dir="rtl">
    
    <!-- ─── Header ──────────────────────────────────────────────────────────── -->
    <AppPageHeader title="تشخيصات النظام المتقدمة" subtitle="رؤية تشغيلية موجَّهة — يدوية بالكامل، لا تحديثات تلقائية">
      <svelte:fragment slot="actions">
        <div class="flex items-end gap-2">
          <div class="w-32">
            <AppInput
              id="year-input"
              label="السنة"
              type="number"
              bind:value={selectedYear}
              min={2020}
              max={2100}
            />
          </div>
          <AppButton
            variant="primary"
            loading={$loading}
            on:click={refresh}
          >
            تحديث يدوي
          </AppButton>
          <AppButton
            variant="secondary"
            disabled={$loading || selectedYear === null}
            on:click={takeSnapshot}
            ariaLabel="إنشاء لقطة تشغيلية يدوية"
          >
            لقطة جديدة
          </AppButton>
        </div>
      </svelte:fragment>
    </AppPageHeader>

    {#if lastRefreshedAt}
      <div class="text-xs text-gray-500 dark:text-gray-400">آخر تحديث: {lastRefreshedAt}</div>
    {/if}

    {#if $error}
      <AppAlert intent="danger" dismissible on:dismiss={() => error.set(null)}>
        <div class="font-semibold">خطأ:</div>
        <div class="text-sm">{$error}</div>
      </AppAlert>
    {/if}

    <!-- ─── Anomaly Findings ────────────────────────────────────────────────── -->
    {#if bundle}
      <AppSection title="🔍 الاكتشافات التشغيلية ({bundle.anomalyReport.findings.length})">
        {#if bundle.anomalyReport.findings.length === 0}
          <AppAlert intent="success">
            لم يتم رصد أي شذوذ تشغيلي.
          </AppAlert>
        {:else}
          <div class="space-y-2">
            {#each bundle.anomalyReport.findings as f}
              <AppAlert intent={severityIntent(f.severity)}>
                <div class="flex items-center justify-between mb-1">
                  <span class="font-mono text-xs">{translateCode(f.code)}</span>
                  <AppBadge intent={severityIntent(f.severity)} size="sm">{translateSeverity(f.severity)}</AppBadge>
                </div>
                <div class="text-sm font-medium">{f.message}</div>
                <div class="text-xs mt-1 opacity-80">
                  <strong>توصية:</strong> {f.recommendation}
                </div>
              </AppAlert>
            {/each}
          </div>
        {/if}
      </AppSection>

      <!-- ─── Recommendations ───────────────────────────────────────────────── -->
      <AppSection title="💡 توصيات تشغيلية ({bundle.recommendations.length})">
        {#if bundle.recommendations.length === 0}
          <p class="text-sm text-gray-600 dark:text-gray-400">لا توجد توصيات نشطة حالياً.</p>
        {:else}
          <div class="space-y-2">
            {#each bundle.recommendations as r}
              <AppCard padding="sm">
                <div class="flex items-center justify-between">
                  <div class="font-medium text-gray-900 dark:text-white">{r.title}</div>
                  <AppBadge intent={priorityIntent(r.priority)} size="sm">
                    {translatePriority(r.priority)}
                  </AppBadge>
                </div>
                <div class="text-sm text-gray-700 dark:text-gray-300 mt-1">{r.message}</div>
                <div class="font-mono text-[10px] text-gray-400 dark:text-gray-500 mt-1">{translateCode(r.code)}</div>
              </AppCard>
            {/each}
          </div>
        {/if}
      </AppSection>

      <!-- ─── Fiscal Integrity Summary ──────────────────────────────────────── -->
      <AppSection title="⚖️ سلامة المالية">
        <AppCard padding="sm">
          <div class="font-medium text-gray-900 dark:text-white mb-2">
            الحالة:
            <AppBadge intent={bundle.fiscalIntegrity.ok ? 'success' : 'warning'}>
              {bundle.fiscalIntegrity.ok ? 'سليمة' : 'تحذيرات'}
            </AppBadge>
          </div>
          {#if bundle.fiscalIntegrity.warnings.length > 0}
            <ul class="text-sm list-disc pr-5 space-y-1 text-gray-700 dark:text-gray-300">
              {#each bundle.fiscalIntegrity.warnings as w}
                <li><span class="font-mono text-xs">{w.code}</span> — {w.details}</li>
              {/each}
            </ul>
          {/if}
        </AppCard>
      </AppSection>

      <!-- ─── Inventory Mismatches (existing capability) ────────────────────── -->
      {#if inventory}
        <AppSection title="📦 تطابق المخزون" description="فحص {inventory.checked_products} منتج، {inventory.mismatch_count} اختلاف">
          {#if inventory.mismatch_count === 0}
            <AppAlert intent="success">
              لا توجد اختلافات في المخزون.
            </AppAlert>
          {:else}
            <AppTable>
              <svelte:fragment slot="head">
                <th class="table-header">المنتج</th>
                <th class="table-header">المتوقع</th>
                <th class="table-header">الفعلي</th>
                <th class="table-header">الفرق</th>
              </svelte:fragment>

              {#each inventory.issues as it}
                <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
                  <td class="table-cell font-mono text-xs">{it.product_id}</td>
                  <td class="table-cell">{it.expected_quantity}</td>
                  <td class="table-cell">{it.actual_quantity}</td>
                  <td class="table-cell text-red-700 dark:text-red-400 font-medium">{it.delta.toFixed(2)}</td>
                </tr>
              {/each}
            </AppTable>
          {/if}
        </AppSection>
      {/if}

      <!-- ─── Operational Snapshots / Growth Analysis ───────────────────────── -->
      <AppSection title="📈 اللقطات التشغيلية ({bundle.recentSnapshots.length})">
        {#if bundle.recentSnapshots.length === 0}
          <AppEmptyState
            title="لا توجد لقطات"
            description="اضغط «لقطة جديدة» لإنشاء الأولى."
          />
        {:else}
          <AppTable>
            <svelte:fragment slot="head">
              <th class="table-header">التاريخ</th>
              <th class="table-header">السنة</th>
              <th class="table-header">قيمة المخزون</th>
              <th class="table-header">المنتجات</th>
              <th class="table-header">الحركات</th>
              <th class="table-header">التقارير</th>
              <th class="table-header">السلامة</th>
              <th class="table-header">بواسطة</th>
            </svelte:fragment>

            {#each bundle.recentSnapshots as s}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
                <td class="table-cell whitespace-nowrap">{s.snapshotDate}</td>
                <td class="table-cell">{s.fiscalYear}</td>
                <td class="table-cell font-mono">{s.totalInventoryValue.toFixed(2)}</td>
                <td class="table-cell">{s.productCount}</td>
                <td class="table-cell">{s.movementCount}</td>
                <td class="table-cell">{s.reportCount}</td>
                <td class="table-cell">
                  <AppBadge intent={integrityIntent(s.integrityState)} size="sm">
                    {s.integrityState}
                  </AppBadge>
                </td>
                <td class="table-cell text-xs text-gray-600 dark:text-gray-400">{s.createdBy}</td>
              </tr>
            {/each}
          </AppTable>
        {/if}
      </AppSection>

      <!-- ─── Fiscal Timeline ───────────────────────────────────────────────── -->
      <AppSection title="🕒 الخط الزمني الموحَّد ({bundle.timeline.length})">
        {#if bundle.timeline.length === 0}
          <p class="text-sm text-gray-600 dark:text-gray-400">لا توجد أحداث مسجّلة في النطاق المحدّد.</p>
        {:else}
          <AppCard padding="none">
            <ul class="divide-y divide-gray-200 dark:divide-gray-700">
              {#each bundle.timeline as ev}
                <li class="p-3 hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors">
                  <div class="flex items-center justify-between">
                    <span class="font-mono text-xs text-gray-500 dark:text-gray-400">{formatTs(ev.timestamp)}</span>
                    <span class="text-xs px-2 py-0.5 rounded bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300">
                      {kindLabel(ev.kind)}
                    </span>
                  </div>
                  <div class="text-sm mt-1 font-medium">{ev.summary}</div>
                  <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                    {#if ev.actor}بواسطة {ev.actor} — {/if}
                    مصدر: {ev.source}
                    {#if ev.fiscalYear !== null} — سنة {ev.fiscalYear}{/if}
                  </div>
                </li>
              {/each}
            </ul>
          </AppCard>
        {/if}
      </AppSection>

      <!-- ─── Backup Health Summary (from existing system health) ───────────── -->
      {#if systemHealth}
        <AppSection title="💾 صحة النسخ الاحتياطية والقاعدة">
          <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
            <AppCard padding="sm">
              <div class="text-xs uppercase text-gray-500 dark:text-gray-400 mb-1">قاعدة البيانات</div>
              <AppBadge intent={systemHealth.databaseStatus.status === 'HEALTHY' ? 'success' : 'danger'}>
                {translateStatus(systemHealth.databaseStatus.status)}
              </AppBadge>
              <div class="font-medium mt-2 text-sm">{systemHealth.databaseStatus.message}</div>
              <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">الحجم: {bytesToMb(systemHealth.storageUsageBytes)} MB</div>
            </AppCard>
            
            <AppCard padding="sm">
              <div class="text-xs uppercase text-gray-500 dark:text-gray-400 mb-1">النسخ الاحتياطية</div>
              <AppBadge intent={systemHealth.backupStatus.status === 'HEALTHY' ? 'success' : 'danger'}>
                {translateStatus(systemHealth.backupStatus.status)}
              </AppBadge>
              <div class="font-medium mt-2 text-sm">{systemHealth.backupStatus.message}</div>
              {#if systemHealth.backupStatus.lastBackup}
                <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">آخر نسخة: {formatTs(systemHealth.backupStatus.lastBackup)}</div>
              {/if}
            </AppCard>
          </div>
        </AppSection>
      {/if}
    {/if}
  </div>
</Layout>
