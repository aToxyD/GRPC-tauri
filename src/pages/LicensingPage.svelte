<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    getLicensingStatus,
    importTrustAnchor,
    importLicense,
    dryRunVerifyLicense,
    verifyLicense,
    type LicensingStatusDto,
    type ImportAnchorResultDto,
    type ImportLicenseResultDto,
  } from '../lib/contracts';
  import { getSettings } from '../lib/contracts';
  import type { Settings } from '../lib/types';
  import { openFile, readTextFile } from '../lib/tauri';
  import Layout from '../components/Layout.svelte';
  import { formatErrorMessage } from '../lib/errors';
  import { showSuccess } from '../lib/notifications';
  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation } from '../lib/operationGuard';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppTextarea from '../lib/components/ui/AppTextarea.svelte';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const statusOp = createOperation({ scope });
  const importOp = createOperation({ scope });
  const verifyOp = createOperation({ scope });
  const statusLoading = statusOp.loading;
  const statusError = statusOp.error;
  const importLoading = importOp.loading;
  const verifyLoading = verifyOp.loading;

  // @category ProjectionState
  let status: LicensingStatusDto | null = null;
  // @category ProjectionState
  let settings: Settings | null = null;

  // @category TransientState
  let anchorJson = '';
  // @category TransientState
  let licenseJson = '';
  // @category TransientState
  let lastImportResult: ImportAnchorResultDto | null = null;
  // @category TransientState
  let lastLicenseResult: ImportLicenseResultDto | null = null;
  // @category TransientState
  let verifyReport: { checked: number; enforceable: number } | null = null;

  async function loadStatus() {
    await statusOp.run(async () => {
      status = await getLicensingStatus();
    });
  }

  async function loadFileInto(target: 'anchor' | 'license') {
    const selected = await openFile({
      filters: [{ name: 'ملف JSON', extensions: ['json'] }],
    });
    if (!selected) return;
    const content = await readTextFile(selected as string);
    if (target === 'anchor') {
      anchorJson = content;
    } else {
      licenseJson = content;
    }
  }

  async function handleImportAnchor() {
    lastImportResult = null;
    lastLicenseResult = null;
    if (!anchorJson.trim()) return;
    await importOp.run(async () => {
      lastImportResult = await importTrustAnchor(anchorJson.trim());
      showSuccess('تم تثبيت مرساة الثقة بنجاح.');
      anchorJson = '';
      await loadStatus();
    });
  }

  async function handleImportLicense() {
    lastLicenseResult = null;
    lastImportResult = null;
    if (!licenseJson.trim()) return;
    await importOp.run(async () => {
      lastLicenseResult = await importLicense(licenseJson.trim());
      showSuccess('تم استيراد رخصة الترخيص.');
      licenseJson = '';
      await loadStatus();
    });
  }

  async function handleDryRun() {
    lastLicenseResult = null;
    if (!licenseJson.trim()) return;
    await verifyOp.run(async () => {
      lastLicenseResult = await dryRunVerifyLicense(licenseJson.trim());
    });
  }

  async function handleVerifyAll() {
    await verifyOp.run(async () => {
      const report = await verifyLicense();
      verifyReport = { checked: report.checked, enforceable: report.enforceable };
      showSuccess('تم التحقق من جميع الرخص.');
      await loadStatus();
    });
  }

  onMount(async () => {
    try {
      settings = await getSettings();
    } catch (_e) {
      /* optional settings */
    }
    await loadStatus();
  });

  // @category UiState
  $: nodeType = (settings?.node_type === 'WILAYA' ? 'WILAYA' : settings?.node_type === 'UNIT' ? 'UNIT' : null) as 'WILAYA' | 'UNIT' | null;
  // @category UiState
  $: gateActive = status?.summary.gate_active === true;
  // @category UiState
  $: enforceableCount = status?.licenses.filter((l) => l.enforceable).length ?? 0;

  function fmt(ts: string | null): string {
    return ts ? new Date(ts).toLocaleString('ar-DZ') : '—';
  }
</script>

<Layout {nodeType} title="الترخيص" subtitle="إدارة ترخيص التشغيل (ADR-0042)">
  <div dir="rtl">
    <AppPageHeader title="الترخيص" subtitle="مرساة الثقة والرخص وحالة البوابة">
      <svelte:fragment slot="actions">
        <AppButton variant="primary" on:click={loadStatus} disabled={$statusLoading}>⟳ تحديث</AppButton>
      </svelte:fragment>
    </AppPageHeader>

    {#if $statusError}
      <div class="mb-4">
        <AppAlert intent="danger">{$statusError}</AppAlert>
      </div>
    {/if}

    {#if $statusLoading && !status}
      <AppLoadingState message="جارٍ تحميل حالة الترخيص..." />
    {:else if status}
      <!-- Status Cards -->
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <AppCard class="border-2 text-center {gateActive ? 'border-green-300 dark:border-green-800' : 'border-gray-200 dark:border-gray-700'}" padding="sm">
          <div class="text-3xl mb-1.5">{gateActive ? '🟢' : '⚪'}</div>
          <div class="font-bold text-base dark:text-white">بوابة الترخيص</div>
          <div class="text-xs mt-1 dark:text-gray-300">
            {gateActive ? 'نشطة — القيود مفروضة' : 'خاملة — لا توجد مرساة ثقة'}
          </div>
        </AppCard>

        <AppCard class="border-2 text-center {status.anchor.installed ? 'border-green-300 dark:border-green-800' : 'border-yellow-300 dark:border-yellow-800'}" padding="sm">
          <div class="text-3xl mb-1.5">{status.anchor.installed ? '🔑' : '🚫'}</div>
          <div class="font-bold text-base dark:text-white">مرساة الثقة</div>
          <div class="text-xs mt-1 dark:text-gray-300">{status.anchor.installed ? status.anchor.key_id : 'غير مثبتة'}</div>
        </AppCard>

        <AppCard class="border-2 text-center border-gray-200 dark:border-gray-700" padding="sm">
          <div class="text-3xl mb-1.5">📄</div>
          <div class="font-bold text-base dark:text-white">الرخص المستوردة</div>
          <div class="text-xs mt-1 dark:text-gray-300">{status.summary.license_count}</div>
        </AppCard>

        <AppCard class="border-2 text-center border-gray-200 dark:border-gray-700" padding="sm">
          <div class="text-3xl mb-1.5">✅</div>
          <div class="font-bold text-base dark:text-white">رخص نافذة</div>
          <div class="text-xs mt-1 dark:text-gray-300">{enforceableCount} / {status.summary.active_license_count}</div>
        </AppCard>
      </div>

      {#if verifyReport}
        <div class="mb-6">
          <AppAlert intent="info">
            نتيجة التحقق: {verifyReport.enforceable} من أصل {verifyReport.checked} رخصة نافذة وقابلة للتنفيذ.
          </AppAlert>
        </div>
      {/if}

      <!-- Trust Anchor Management -->
      <AppCard class="mb-6">
        <div class="flex items-center justify-between mb-4">
          <h2 class="font-semibold text-gray-800 dark:text-white">🔑 مرساة الثقة (Trust Anchor)</h2>
          <AppButton variant="secondary" size="sm" on:click={() => loadFileInto('anchor')}>تحميل من ملف…</AppButton>
        </div>
        <AppTextarea
          id="anchor-package-json"
          label="حزمة التزويد (provisioning-v1) — JSON"
          bind:value={anchorJson}
          placeholder={'{"schema":"provisioning-v1", ...}'}
          rows={5}
          helperText="تثبيت مرساة جديدة أو تدوير المرساة النشطة."
        />
        <div class="mt-3 flex items-center gap-3">
          <AppButton variant="primary" on:click={handleImportAnchor} loading={$importLoading} disabled={!anchorJson.trim()}>
            تثبيت / تدوير المرساة
          </AppButton>
        </div>
        {#if lastImportResult}
          <div class="mt-3">
            <AppAlert intent={lastImportResult.installed ? 'success' : 'danger'}>
              {lastImportResult.installed
                ? `تم تثبيت المرساة ${lastImportResult.key_id}${lastImportResult.replaced_key_id ? ' (استُبدلت: ' + lastImportResult.replaced_key_id + ')' : ''}.`
                : 'فشل تثبيت المرساة.'}
            </AppAlert>
          </div>
        {/if}
      </AppCard>

      <!-- License Import -->
      <AppCard class="mb-6">
        <div class="flex items-center justify-between mb-4">
          <h2 class="font-semibold text-gray-800 dark:text-white">📄 استيراد رخصة (License Artifact)</h2>
          <AppButton variant="secondary" size="sm" on:click={() => loadFileInto('license')}>تحميل من ملف…</AppButton>
        </div>
        <AppTextarea
          id="license-artifact-json"
          label="مصنّف الرخصة الموقّع — JSON"
          bind:value={licenseJson}
          placeholder={'{"version":1, "metadata":{...}, "payload":{...}, "signature":{...}}'}
          rows={5}
          helperText="الاستيراد يشغّل خط الأنابيب الكامل (البنية، الإصدار، الهوية، التوقيع، ربط العقدة)."
        />
        <div class="mt-3 flex items-center gap-3">
          <AppButton variant="primary" on:click={handleImportLicense} loading={$importLoading} disabled={!licenseJson.trim()}>
            استيراد الرخصة
          </AppButton>
          <AppButton variant="secondary" on:click={handleDryRun} loading={$verifyLoading} disabled={!licenseJson.trim()}>
            تحقق أولي (بدون حفظ)
          </AppButton>
        </div>
        {#if lastLicenseResult}
          <div class="mt-3">
            <AppAlert intent={lastLicenseResult.outcome === 'imported' ? 'success' : lastLicenseResult.outcome === 'not-for-this-node' ? 'warning' : 'danger'}>
              <p class="font-semibold">{lastLicenseResult.message}</p>
              {#if lastLicenseResult.outcome === 'imported'}
                <p class="text-xs mt-1">معرف الرخصة: {lastLicenseResult.license_id} — المعرف القطعي: {lastLicenseResult.artifact_id}</p>
              {/if}
            </AppAlert>
          </div>
        {/if}
      </AppCard>

      <!-- Licenses Table -->
      <div class="flex items-center justify-between mb-3">
        <h2 class="font-semibold text-gray-800 dark:text-white">الرخص ({status.licenses.length})</h2>
        <AppButton variant="primary" on:click={handleVerifyAll} loading={$verifyLoading}>
          🔍 إعادة التحقق من الكل
        </AppButton>
      </div>

      {#if status.licenses.length > 0}
        <AppCard padding="none" class="mb-6">
          <AppTable>
            <svelte:fragment slot="head">
              <th class="table-header text-right">النوع</th>
              <th class="table-header text-right">المعرف القطعي</th>
              <th class="table-header text-right">الجهة المخوّلة</th>
              <th class="table-header text-right">الامتيازات</th>
              <th class="table-header text-right">الحالة</th>
              <th class="table-header text-right">قابلة للتنفيذ</th>
              <th class="table-header text-right">آخر تحقق</th>
            </svelte:fragment>

            {#each status.licenses as license}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                <td class="table-cell"><code class="dark:text-gray-300">{license.type_key}</code></td>
                <td class="table-cell"><code class="text-xs dark:text-gray-300">{license.artifact_id.slice(0, 12)}…</code></td>
                <td class="table-cell dark:text-gray-300">{license.subject_id}</td>
                <td class="table-cell text-xs dark:text-gray-400">{license.entitlements.join(', ') || '—'}</td>
                <td class="table-cell">
                  <AppBadge intent={license.status === 'active' ? 'success' : 'warning'} size="sm">{license.status}</AppBadge>
                </td>
                <td class="table-cell">
                  {#if license.enforceable}
                    <AppBadge intent="success" size="sm">نعم</AppBadge>
                  {:else}
                    <AppBadge intent="danger" size="sm">لا</AppBadge>
                  {/if}
                </td>
                <td class="table-cell text-xs dark:text-gray-400">{fmt(license.last_verified_at)}</td>
              </tr>
            {/each}
          </AppTable>
        </AppCard>
      {:else}
        <AppCard class="mb-6">
          <p class="text-center text-gray-500 dark:text-gray-400 py-6 text-sm">لا توجد رخص مستوردة بعد.</p>
        </AppCard>
      {/if}
    {/if}
  </div>
</Layout>
