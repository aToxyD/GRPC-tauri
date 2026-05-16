<script lang="ts">
  import { onMount } from 'svelte';
  import { save, open } from '@tauri-apps/plugin-dialog';
  import { 
    closeFiscalYear, 
    getFiscalYearStatus, 
    listProducts,
    getSettings,
    exportFiscalClosurePackage,
    previewFiscalClosurePackage,
    applyFiscalClosurePackage,
    getFiscalTransitionHistory,
    listFiscalPackageRegistry,
    updateFiscalPackageRetentionStatus
  } from '../lib/tauri';
  import type { 
    FiscalYearStatus, 
    FiscalClosurePreview, 
    FiscalClosureApplyResult,
    FiscalTransitionHistoryEntry,
    FiscalPackageRegistryEntry
  } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let nodeType: 'WILAYA' | 'UNIT' | null = null;
  let currentYear = new Date().getFullYear();
  let nextYear = currentYear + 1;
  let confirmation = '';
  let loading = false;
  let message = '';
  let productCount = 0;
  let fiscalStatus: FiscalYearStatus | null = null;

  // Import/Preview state
  let preview: FiscalClosurePreview | null = null;
  let selectedFilePath = '';
  let applyConfirmation = '';
  
  // History & Registry state
  let transitionHistory: FiscalTransitionHistoryEntry[] = [];
  let packageRegistry: FiscalPackageRegistryEntry[] = [];
  let retentionConfirmation = '';
  let selectedPackageForRetention: FiscalPackageRegistryEntry | null = null;
  let targetRetentionStatus: 'ARCHIVED' | 'RETIRED' | null = null;

  async function load() {
    const settings = await getSettings();
    nodeType = settings.node_type;
    
    fiscalStatus = await getFiscalYearStatus(settings.current_year);
    if (fiscalStatus) {
      currentYear = fiscalStatus.year;
      nextYear = currentYear + 1;
    } else {
      currentYear = settings.current_year;
      nextYear = currentYear + 1;
    }

    const products = await listProducts();
    productCount = products.length;

    await refreshHistory();
  }

  async function refreshHistory() {
    try {
      transitionHistory = await getFiscalTransitionHistory();
      packageRegistry = await listFiscalPackageRegistry();
    } catch (e) {
      console.error('Failed to load history:', e);
    }
  }

  async function handleUpdateRetention(transitionId: string, status: 'ARCHIVED' | 'RETIRED') {
    const expected = `${status}-PACKAGE`;
    if (retentionConfirmation !== expected) {
      message = `يرجى كتابة رمز التأكيد ${expected} بشكل صحيح.`;
      return;
    }

    loading = true;
    try {
      await updateFiscalPackageRetentionStatus(transitionId, status, retentionConfirmation);
      message = `تم تحديث حالة الحزمة إلى ${status} بنجاح.`;
      selectedPackageForRetention = null;
      retentionConfirmation = '';
      await refreshHistory();
    } catch (error) {
      message = String(error);
    } finally {
      loading = false;
    }
  }

  async function executeClose() {
    if (confirmation !== String(currentYear)) {
      message = 'رقم السنة غير صحيح. تم رفض العملية.';
      return;
    }

    const confirmed = confirm(`سيتم إغلاق السنة المالية ${currentYear} وفتح ${nextYear}. لا يمكن التراجع عن العملية.`);
    if (!confirmed) return;

    loading = true;
    message = '';

    try {
      const result = await closeFiscalYear({
        year: currentYear,
        next_year: nextYear,
      });

      message = `تم إغلاق ${result.closed_year} وفتح ${result.opened_year} بنجاح.`;
      await load(); // Reload state
      confirmation = '';
      await refreshHistory();
    } catch (error) {
      message = String(error);
    } finally {
      loading = false;
    }
  }

  async function handleExportPackage() {
    if (!fiscalStatus || fiscalStatus.status === 'open') {
      message = 'يجب إغلاق السنة المالية أولاً قبل تصدير حزمة الترخيص.';
      return;
    }
    await handleExportRegistryPackage(fiscalStatus.year, fiscalStatus.year + 1, fiscalStatus.closed_at || new Date().toISOString());
  }

  async function handleExportRegistryPackage(closedYear: number, openedYear: number, timestamp: string, transitionId: string | null = null) {
    const filePath = await save({
      title: 'حفظ حزمة ترخيص إغلاق السنة المالية',
      defaultPath: `fiscal_closure_${closedYear}.fiscal-close.sync`,
      filters: [{ name: 'Fiscal Closure Package', extensions: ['sync'] }]
    });

    if (!filePath) return;

    loading = true;
    try {
      await exportFiscalClosurePackage(
        closedYear,
        openedYear,
        timestamp,
        filePath,
        transitionId
      );
      message = `تم تصدير حزمة الترخيص للسنة ${closedYear} بنجاح إلى: ${filePath}`;
      await refreshHistory();
    } catch (error) {
      message = String(error);
    } finally {
      loading = false;
    }
  }

  async function handleSelectPackage() {
    const filePath = await open({
      title: 'اختر حزمة ترخيص إغلاق السنة المالية',
      multiple: false,
      filters: [{ name: 'Fiscal Closure Package', extensions: ['sync'] }]
    });

    if (!filePath || Array.isArray(filePath)) return;

    selectedFilePath = filePath;
    loading = true;
    preview = null;
    message = '';

    try {
      preview = await previewFiscalClosurePackage(selectedFilePath);
      if (!preview.validation_ok) {
        message = 'فشل التحقق المسبق من الحزمة. راجع التفاصيل أدناه.';
      }
    } catch (error) {
      message = String(error);
      selectedFilePath = '';
    } finally {
      loading = false;
    }
  }

  async function handleApplyPackage() {
    if (!preview || !preview.validation_ok) return;
    if (applyConfirmation !== 'APPLY-FISCAL-TRANSITION') {
      message = 'يرجى كتابة رمز التأكيد بشكل صحيح.';
      return;
    }

    loading = true;
    message = '';

    try {
      const result = await applyFiscalClosurePackage(selectedFilePath, applyConfirmation);
      message = `تم تطبيق الانتقال المالي للسنة ${result.closed_year} بنجاح. تم ترحيل ${result.snapshot_count} سجل.`;
      preview = null;
      selectedFilePath = '';
      applyConfirmation = '';
      await load();
      await refreshHistory();
    } catch (error) {
      message = String(error);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  // Helper to determine badge state
  $: isExpired = preview && new Date() > new Date(preview.authorized_execution_window.expires_at);
  $: isNotYetStarted = preview && new Date() < new Date(preview.authorized_execution_window.not_before);
  $: isReplay = preview && preview.validation_issues.some(i => i.includes('replay'));
  $: isInvalidSignature = message.includes('HMAC verification failed');
</script>

<Layout {nodeType} title="إدارة السنة المالية">
  <div class="max-w-3xl space-y-6">
    <div class="flex justify-between items-center bg-white p-4 rounded-2xl shadow-sm border">
      <div>
        <p class="text-sm text-gray-500">السنة المالية المفتوحة حالياً</p>
        <p class="text-2xl font-bold text-gray-800">{currentYear}</p>
      </div>
      <div class="text-right">
        <p class="text-sm text-gray-500">حالة النظام</p>
        <span class="px-3 py-1 rounded-full text-xs font-medium {fiscalStatus?.status === 'open' ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}">
          {fiscalStatus?.status === 'open' ? 'مفتوح للعمليات' : 'مغلق مالياً'}
        </span>
      </div>
    </div>

    {#if nodeType === 'WILAYA'}
      <!-- WILAYA MODE: CLOSE & EXPORT -->
      {#if fiscalStatus?.status === 'open'}
        <div class="border border-red-400 bg-red-50 rounded-2xl p-4 text-red-800">
          <h2 class="font-bold mb-2">سلطة إغلاق السنة المالية (WILAYA)</h2>
          <p>أنت تملك سلطة اتخاذ قرار الإغلاق المالي. سيؤدي هذا الإجراء إلى:</p>
          <ul class="list-disc pr-5 mt-2 space-y-1 text-sm">
            <li>قفل السنة المالية {currentYear} نهائياً</li>
            <li>توليد حزمة ترخيص للانتقال المالي للعقد التابعة</li>
            <li>فتح السنة المالية الجديدة {nextYear}</li>
          </ul>
          <p class="text-xs mt-3 bg-white/50 p-2 rounded text-red-900 border border-red-200">
            <strong>ملاحظة (Operator Guide):</strong> الغلق المالي هو عملية لا رجعة فيها (Irreversible). التواريخ والبصمات الناتجة ستكون مسجلة بشكل دائم في سلسلة التدقيق المغلقة. التوكيد النصي مطلوب لمنع الغلق الخطأ.
          </p>
        </div>

        <div class="border rounded-2xl p-4 bg-white shadow-sm space-y-4">
          <h2 class="font-semibold border-b pb-2">تنفيذ قرار الإغلاق</h2>
          <div class="grid grid-cols-2 gap-3 text-sm mb-4 bg-gray-50 p-3 rounded-xl">
            <div>السنة الحالية: <span class="font-mono font-bold">{currentYear}</span></div>
            <div>السنة الجديدة: <span class="font-mono font-bold">{nextYear}</span></div>
          </div>

          <label class="block text-sm font-medium" for="year-confirmation">
            اكتب رقم السنة الحالية ({currentYear}) للتأكيد
          </label>

          <input
            id="year-confirmation"
            bind:value={confirmation}
            class="w-full border rounded-xl px-4 py-2 focus:ring-2 focus:ring-red-500 focus:outline-none"
            placeholder={String(currentYear)}
          />

          <button
            on:click={executeClose}
            disabled={loading}
            class="w-full px-4 py-3 rounded-xl bg-red-600 text-white font-bold hover:bg-red-700 transition-colors disabled:opacity-50"
          >
            {#if loading}
              جاري تنفيذ الإغلاق...
            {:else}
              إصدار قرار إغلاق السنة {currentYear}
            {/if}
          </button>
        </div>
      {:else}
        <!-- ALREADY CLOSED IN WILAYA: ALLOW EXPORT -->
        <div class="border border-blue-200 bg-blue-50 rounded-2xl p-6 text-center space-y-4 shadow-sm">
          <div class="mx-auto w-16 h-16 bg-blue-100 rounded-full flex items-center justify-center text-blue-600 mb-2">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04a11.367 11.367 0 01-1.091 5.496c.002.314.05.628.143.933a11.503 11.503 0 001.371 3.513c.176.326.362.641.551.944A12.026 12.026 0 0011.962 21.01a12.02 12.02 0 008.474-5.991c.401-.736.745-1.515 1.022-2.322a10.107 10.107 0 00.395-1.842 11.233 11.233 0 00-1.091-5.496z" />
            </svg>
          </div>
          <h2 class="text-xl font-bold text-blue-900">تصدير ترخيص الانتقال</h2>
          <p class="text-blue-700">السنة المالية {fiscalStatus?.year} مغلقة. يمكنك تصدير حزمة ترخيص الانتقال المالي (Remote Authorization Package).</p>
          
          <button
            on:click={handleExportPackage}
            disabled={loading}
            class="px-8 py-3 rounded-xl bg-blue-600 text-white font-bold hover:bg-blue-700 shadow-lg shadow-blue-200 transition-all disabled:opacity-50"
          >
            تصدير حزمة الترخيص (.sync)
          </button>
        </div>
      {/if}
    {:else}
      <!-- UNIT MODE: IMPORT & APPLY -->
      <div class="border rounded-2xl p-6 bg-white shadow-sm space-y-6">
        <div class="flex items-start space-x-4 space-x-reverse">
          <div class="bg-indigo-100 p-3 rounded-xl text-indigo-600">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
            </svg>
          </div>
          <div class="flex-1">
            <h2 class="text-lg font-bold">تطبيق انتقال مالي مُصرّح من الولاية</h2>
            <p class="text-sm text-gray-600">أنت تقوم بتنفيذ قرار إغلاق اتخذته الولاية مسبقاً. لا تملك الوحدات (UNIT) سلطة إغلاق مستقلة.</p>
            <p class="text-xs text-indigo-700 bg-indigo-50 p-2 mt-2 rounded border border-indigo-200">
              <strong>ملاحظة أمان:</strong> الحزم محمية ضد إعادة التشغيل (Replay Protection). إذا تم تطبيق حزمة مسبقاً، سيتم رفضها منعاً لتكرار ترحيل الأرصدة.
            </p>
          </div>
        </div>

        {#if !preview}
          <button
            on:click={handleSelectPackage}
            disabled={loading}
            class="w-full py-8 border-2 border-dashed border-gray-200 rounded-2xl text-gray-400 hover:bg-gray-50 hover:border-indigo-400 hover:text-indigo-600 transition-all flex flex-col items-center justify-center space-y-2"
          >
            <span class="font-medium text-lg">استيراد حزمة الانتقال (.sync)</span>
            <span class="text-xs">المستخرجة من عقد الولاية</span>
          </button>
        {:else}
          <!-- PREVIEW PANEL -->
          <div class="bg-gray-50 border border-gray-200 rounded-2xl p-5 space-y-5">
            <div class="flex justify-between items-center border-b pb-3">
              <h3 class="font-bold text-gray-900">مراجعة ترخيص الانتقال</h3>
              <button on:click={() => preview = null} class="text-xs text-indigo-600 hover:underline">تغيير الملف</button>
            </div>
            
            <div class="grid grid-cols-2 gap-y-4 gap-x-6 text-sm">
              <div>
                <p class="text-gray-400 text-xs">الانتقال المالي</p>
                <p class="font-bold text-gray-800">{preview.closed_year} ← {preview.opened_year}</p>
              </div>
              <div>
                <p class="text-gray-400 text-xs">مصدر السلطة (Wilaya)</p>
                <p class="font-bold text-gray-800">{preview.closure_authority_node_id}</p>
                <p class="text-[10px] text-gray-500">{preview.closure_authority_username}</p>
              </div>
              <div class="col-span-2">
                <p class="text-gray-400 text-xs">نافذة التنفيذ المصرح بها</p>
                <div class="flex items-center space-x-2 space-x-reverse mt-1">
                  <span class="text-[10px] font-mono bg-white border px-2 py-1 rounded">من: {new Date(preview.authorized_execution_window.not_before).toLocaleString()}</span>
                  <span class="text-[10px] font-mono bg-white border px-2 py-1 rounded">إلى: {new Date(preview.authorized_execution_window.expires_at).toLocaleString()}</span>
                </div>
              </div>
              <div class="col-span-2">
                <p class="text-gray-400 text-xs">معرف الانتقال (Transition ID)</p>
                <code class="text-[10px] block bg-white p-2 border rounded mt-1 break-all font-mono">{preview.fiscal_transition_id}</code>
              </div>
              <div class="col-span-2">
                <p class="text-gray-400 text-xs">بصمة الحزمة (Fingerprint)</p>
                <code class="text-[10px] block bg-white p-2 border rounded mt-1 break-all font-mono font-bold text-indigo-700">{preview.package_fingerprint}</code>
              </div>
              <div>
                <p class="text-gray-400 text-xs">معرف مفتاح التوقيع</p>
                <p class="font-mono text-[10px]">{preview.signing_key_id || 'DEFAULT'}</p>
              </div>
              <div class="text-left">
                 <p class="text-gray-400 text-xs">حالة التحقق</p>
                 {#if isInvalidSignature}
                    <span class="bg-red-100 text-red-700 px-2 py-0.5 rounded text-[10px] font-bold">INVALID SIGNATURE</span>
                 {:else if isReplay}
                    <span class="bg-red-100 text-red-700 px-2 py-0.5 rounded text-[10px] font-bold">REPLAY BLOCKED</span>
                 {:else if isExpired}
                    <span class="bg-red-100 text-red-700 px-2 py-0.5 rounded text-[10px] font-bold">EXPIRED</span>
                 {:else if isNotYetStarted}
                    <span class="bg-amber-100 text-amber-700 px-2 py-0.5 rounded text-[10px] font-bold">NOT YET AUTHORIZED</span>
                 {:else if preview.validation_ok}
                    <span class="bg-green-100 text-green-700 px-2 py-0.5 rounded text-[10px] font-bold">VALID</span>
                 {:else}
                    <span class="bg-red-100 text-red-700 px-2 py-0.5 rounded text-[10px] font-bold">INVALID</span>
                 {/if}
              </div>
            </div>

            {#if !preview.validation_ok}
              <div class="bg-red-50 border border-red-100 text-red-800 p-3 rounded-xl text-xs">
                <p class="font-bold mb-1">فشل التحقق التشغيلي:</p>
                <ul class="list-disc pr-4 space-y-1">
                  {#each preview.validation_issues as issue}
                    <li>{issue}</li>
                  {/each}
                </ul>
              </div>
            {:else}
              <div class="space-y-3 pt-2 border-t mt-2">
                <div class="flex items-center space-x-2 space-x-reverse bg-green-50 text-green-700 p-2 rounded text-[11px]">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
                  </svg>
                  <span>الحزمة سليمة ومصرح بتنفيذها ضمن النافذة الزمنية.</span>
                </div>
                
                <label class="block text-xs font-medium text-gray-700" for="apply-confirm">
                  لتأكيد التنفيذ المحلي، اكتب: <span class="font-mono font-bold select-all text-indigo-600">APPLY-FISCAL-TRANSITION</span>
                </label>
                <input
                  id="apply-confirm"
                  bind:value={applyConfirmation}
                  class="w-full border rounded-xl px-4 py-2 focus:ring-2 focus:ring-indigo-500 focus:outline-none text-sm"
                  placeholder="APPLY-FISCAL-TRANSITION"
                />
                <button
                  on:click={handleApplyPackage}
                  disabled={loading || applyConfirmation !== 'APPLY-FISCAL-TRANSITION'}
                  class="w-full py-3 bg-indigo-600 text-white rounded-xl font-bold shadow-lg shadow-indigo-100 transition-all disabled:opacity-30 disabled:shadow-none"
                >
                  {#if loading}جاري التنفيذ...{:else}تنفيذ الانتقال المالي المصرح به{/if}
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    {#if message}
      <div class="rounded-2xl border p-4 text-sm font-medium {message.includes('بنجاح') ? 'bg-green-50 border-green-200 text-green-800' : 'bg-red-50 border-red-200 text-red-800'}">
        {message}
      </div>
    {/if}

    <!-- Fiscal Transition History Section -->
    <div class="bg-white rounded-2xl shadow-sm border overflow-hidden">
      <div class="p-4 border-b flex justify-between items-center bg-gray-50">
        <h2 class="font-bold text-gray-800">تاريخ الانتقالات المالية</h2>
        <button on:click={refreshHistory} class="text-xs text-indigo-600 hover:bg-indigo-50 px-2 py-1 rounded">تحديث يدوي</button>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-right text-xs">
          <thead>
            <tr class="bg-gray-100 text-gray-500 uppercase tracking-wider">
              <th class="px-4 py-2 border-b">الوقت</th>
              <th class="px-4 py-2 border-b">الإجراء</th>
              <th class="px-4 py-2 border-b">السنة</th>
              <th class="px-4 py-2 border-b">الممثل</th>
              <th class="px-4 py-2 border-b">المصدر</th>
              <th class="px-4 py-2 border-b">الحالة</th>
              <th class="px-4 py-2 border-b">البصمة</th>
            </tr>
          </thead>
          <tbody class="divide-y">
            {#each transitionHistory as entry}
              <tr class="hover:bg-gray-50">
                <td class="px-4 py-2 font-mono text-[10px] whitespace-nowrap">{new Date(entry.timestamp).toLocaleString('ar-DZ')}</td>
                <td class="px-4 py-2">
                   <span class="px-2 py-0.5 rounded-full text-[10px] font-medium {entry.source === 'AUTHORITY' ? 'bg-purple-100 text-purple-700' : 'bg-blue-100 text-blue-700'}">
                    {entry.action}
                   </span>
                </td>
                <td class="px-4 py-2 font-bold">{entry.fiscal_year} → {entry.next_year}</td>
                <td class="px-4 py-2">{entry.actor}</td>
                <td class="px-4 py-2">{entry.source}</td>
                <td class="px-4 py-2">
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-bold 
                    {entry.status.includes('REJECTED') || entry.status.includes('EXPIRED') ? 'bg-red-100 text-red-700' : 'bg-green-100 text-green-700'}">
                    {entry.status}
                  </span>
                </td>
                <td class="px-4 py-2 font-mono text-[10px] text-gray-400" title={entry.package_fingerprint}>
                  {entry.package_fingerprint ? entry.package_fingerprint.substring(0, 8) + '...' : 'N/A'}
                </td>
              </tr>
            {/each}
            {#if transitionHistory.length === 0}
              <tr>
                <td colspan="7" class="px-4 py-8 text-center text-gray-400 italic">لا يوجد سجل انتقالات حالياً</td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>
    </div>

    <!-- Fiscal Closure Package Registry Section -->
    <div class="bg-white rounded-2xl shadow-sm border overflow-hidden">
      <div class="p-4 border-b flex justify-between items-center bg-gray-50">
        <h2 class="font-bold text-gray-800">سجل حزم إغلاق السنة المالية</h2>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-right text-xs">
          <thead>
            <tr class="bg-gray-100 text-gray-500 uppercase tracking-wider">
              <th class="px-4 py-2 border-b">ID الانتقال</th>
              <th class="px-4 py-2 border-b">السنوات</th>
              <th class="px-4 py-2 border-b">البصمة</th>
              <th class="px-4 py-2 border-b">تاريخ التصدير</th>
              <th class="px-4 py-2 border-b">تاريخ التطبيق</th>
              <th class="px-4 py-2 border-b">الحالة</th>
              {#if nodeType === 'WILAYA'}
                <th class="px-4 py-2 border-b text-center">إجراءات الحفظ</th>
              {/if}
            </tr>
          </thead>
          <tbody class="divide-y">
            {#each packageRegistry as pkg}
              <tr class="hover:bg-gray-50">
                <td class="px-4 py-2 font-mono text-[10px] break-all max-w-[120px]">{pkg.transition_id}</td>
                <td class="px-4 py-2 font-bold">{pkg.fiscal_year} → {pkg.next_year}</td>
                <td class="px-4 py-2 font-mono text-[10px] font-bold text-indigo-600">{pkg.package_fingerprint.substring(0, 12)}...</td>
                <td class="px-4 py-2 whitespace-nowrap">{new Date(pkg.exported_at).toLocaleDateString('ar-DZ')}</td>
                <td class="px-4 py-2 whitespace-nowrap">{pkg.applied_at ? new Date(pkg.applied_at).toLocaleDateString('ar-DZ') : '---'}</td>
                <td class="px-4 py-2">
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-bold 
                    {pkg.retention_status === 'ACTIVE' ? 'bg-green-100 text-green-700' : pkg.retention_status === 'ARCHIVED' ? 'bg-amber-100 text-amber-700' : 'bg-gray-100 text-gray-700'}">
                    {pkg.retention_status}
                  </span>
                </td>
                {#if nodeType === 'WILAYA'}
                  <td class="px-4 py-2 text-center space-x-1 space-x-reverse">
                    <button 
                      on:click={() => handleExportRegistryPackage(pkg.fiscal_year, pkg.next_year, pkg.exported_at, pkg.transition_id)}
                      class="text-[10px] bg-blue-50 text-blue-700 border border-blue-200 px-2 py-1 rounded hover:bg-blue-100"
                      title="تصدير ملف .sync للوحدات"
                    >تصدير</button>
                    {#if pkg.retention_status === 'ACTIVE'}
                      <button 
                        on:click={() => { selectedPackageForRetention = pkg; targetRetentionStatus = 'ARCHIVED'; }}
                        class="text-[10px] bg-amber-50 text-amber-700 border border-amber-200 px-2 py-1 rounded hover:bg-amber-100"
                      >أرشفة</button>
                      <button 
                         on:click={() => { selectedPackageForRetention = pkg; targetRetentionStatus = 'RETIRED'; }}
                        class="text-[10px] bg-gray-50 text-gray-700 border border-gray-200 px-2 py-1 rounded hover:bg-gray-100"
                      >استبعاد</button>
                    {/if}
                  </td>
                {/if}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>

    <!-- Retention Confirmation Modal -->
    {#if selectedPackageForRetention && targetRetentionStatus}
      <div class="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4 z-50">
        <div class="bg-white rounded-3xl p-6 max-w-md w-full shadow-2xl space-y-4">
          <h3 class="text-lg font-bold text-gray-900">تأكيد تغيير حالة الاحتفاظ</h3>
          <p class="text-sm text-gray-600">
            أنت على وشك تغيير حالة الحزمة 
            <span class="font-mono font-bold">{selectedPackageForRetention.transition_id}</span>
            إلى <span class="font-bold text-indigo-600">{targetRetentionStatus}</span>.
          </p>
          
          <div class="bg-amber-50 border border-amber-200 p-3 rounded-xl text-xs text-amber-800">
            <strong>ملاحظة:</strong> هذا الإجراء تشغيلي فقط ولا يؤثر على سلامة البيانات المالية، لكنه يحدد تصنيف الحزمة في الأرشيف.
          </div>

          <label class="block text-xs font-medium text-gray-700" for="retention-confirm">
            اكتب الرمز التالي للتأكيد: <span class="font-mono font-bold select-all text-red-600">{targetRetentionStatus}-PACKAGE</span>
          </label>
          <input
            id="retention-confirm"
            bind:value={retentionConfirmation}
            class="w-full border rounded-xl px-4 py-2 focus:ring-2 focus:ring-red-500 focus:outline-none text-sm"
            placeholder={`${targetRetentionStatus}-PACKAGE`}
          />

          <div class="flex space-x-3 space-x-reverse pt-2">
            <button
              on:click={() => handleUpdateRetention(selectedPackageForRetention!.transition_id, targetRetentionStatus!)}
              disabled={loading || retentionConfirmation !== `${targetRetentionStatus}-PACKAGE`}
              class="flex-1 py-2 bg-red-600 text-white rounded-xl font-bold disabled:opacity-30"
            >تأكيد التغيير</button>
            <button
              on:click={() => { selectedPackageForRetention = null; targetRetentionStatus = null; retentionConfirmation = ''; }}
              class="flex-1 py-2 bg-gray-100 text-gray-700 rounded-xl font-bold"
            >إلغاء</button>
          </div>
        </div>
      </div>
    {/if}
  </div>
</Layout>

<style>
  /* Custom glassmorphism and animations if needed */
</style>
