<script lang="ts">
  import { onMount } from 'svelte';
  import {
    importDailyReportPackage,
    importMonthlySummaryPackage,
    listUnits,
    getSettings,
    importStockMovementsPackage
  } from '../lib/tauri';
  import { open } from '@tauri-apps/plugin-dialog';
  import type { Unit, Settings } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let units: Unit[] = [];
  let settings: Settings | null = null;
  let loading = true;
  let error = '';
  let success = '';
  let importProgress = '';
  let selectedUnit: string = '';

  onMount(async () => {
    try {
      settings = await getSettings();
      if (settings?.wilaya_code) {
        units = await listUnits(settings.wilaya_code);
        if (units.length > 0) {
          selectedUnit = units[0].id;
        }
      }
    } catch (e) {
      error = 'خطأ في التحميل: ' + String(e);
    } finally {
      loading = false;
    }
  });

  async function importDailyReportPackageSync() {
    if (!selectedUnit) {
      error = 'الرجاء اختيار وحدة';
      return;
    }

    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'حزمة المزامنة',
          extensions: ['sync']
        }]
      });

      if (selected) {
        importProgress = 'استيراد تقرير يومي مؤمن...';
        error = '';
        success = '';
        const result = await importDailyReportPackage(selected as string, selectedUnit);
        success = `تم استيراد ${result.report_count} تقرير (${result.item_count} عناصر استهلاك)`;
        importProgress = '';
      }
    } catch (e) {
      error = 'خطأ في الاستيراد: ' + String(e);
      importProgress = '';
    }
  }


  async function importMonthlyReportSync() {
    if (!selectedUnit) {
      error = 'يرجى اختيار وحدة';
      return;
    }

    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'حزمة المزامنة',
          extensions: ['sync']
        }]
      });

      if (selected) {
        importProgress = 'استيراد حزمة شهرية مؤمنة...';
        error = '';
        success = '';
        const result = await importMonthlySummaryPackage(selected as string, selectedUnit);
        success = `تم استيراد الملخص الشهري (${result.report_count} تقارير مسجلة في الحزمة)`;
        importProgress = '';
      }
    } catch (e) {
      error = 'خطأ في الاستيراد: ' + String(e);
      importProgress = '';
    }
  }



  async function importStockMovementsPackageSync() {
    if (!selectedUnit) {
      error = 'يرجى اختيار وحدة أولاً';
      return;
    }

    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'حزمة المزامنة',
          extensions: ['sync']
        }]
      });

      if (selected) {
        importProgress = 'استيراد حزمة حركات مؤمنة...';
        error = '';
        success = '';
        const result = await importStockMovementsPackage(selected as string, selectedUnit);
        success = `تم استيراد ${result.movement_count} حركة مخزون بنجاح (المعرف الفريد للحزمة: ${result.file_hash.substring(0, 8)}...)`;
        importProgress = '';
      }
    } catch (e) {
      error = 'خطأ في الاستيراد: ' + String(e);
      importProgress = '';
    }
  }
</script>

<Layout nodeType="WILAYA" title="المزامنة" subtitle="استيراد تقارير الوحدات">

    {#if error}
      <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
        {error}
      </div>
    {/if}

    {#if success}
      <div class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm">
        {success}
      </div>
    {/if}

    {#if importProgress}
      <div class="mb-4 p-3 bg-blue-50 border border-blue-200 rounded-lg text-blue-700 text-sm flex items-center">
        <svg class="animate-spin -ml-1 mr-3 h-5 w-5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        {importProgress}
      </div>
    {/if}

    {#if loading}
      <div class="flex items-center justify-center py-12">
        <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"></div>
      </div>
    {:else if units.length === 0}
      <div class="card text-center py-12 text-gray-500">
        <svg class="w-16 h-16 mx-auto mb-4 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"/>
        </svg>
        <p class="text-lg">لا يوجد وحدات متاحة. أنشئ وحدات أولاً.</p>
        <a href="#/wilaya/units" class="text-civil-blue hover:underline mt-2 inline-block">أنشئ وحدة أولاً</a>
      </div>
    {:else}
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
        <!-- Unit Selection -->
        <div class="card md:col-span-2">
          <h2 class="text-lg font-semibold text-gray-800 mb-4">اختيار الوحدة</h2>
          <p class="text-sm text-gray-600 mb-4">اختر الوحدة التي تستورد تقريرها</p>
          <select class="input-field" bind:value={selectedUnit}>
            {#each units as unit}
              <option value={unit.id}>{unit.code} - {unit.name}</option>
            {/each}
          </select>
        </div>

        <!-- Import Daily Report -->
        <div class="card">
          <div class="flex items-center gap-3 mb-4">
            <div class="w-12 h-12 bg-blue-50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-civil-blue" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800">تقرير يومي</h3>
              <p class="text-sm text-gray-500">حزمة المزامنة (.sync) هي المسار الحالي للمزامنة</p>
            </div>
          </div>
          <div class="space-y-2">
            <button
              on:click={importDailyReportPackageSync}
              class="w-full btn-primary"
              disabled={!selectedUnit || !!importProgress}
              title="هذا هو مسار المزامنة الرسمي بين العقد"
            >
              استيراد حزمة المزامنة (.sync)
            </button>
          </div>
        </div>

        <!-- Import Monthly Report -->
        <div class="card">
          <div class="flex items-center gap-3 mb-4">
            <div class="w-12 h-12 bg-green-50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-green-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800">تقرير شهر</h3>
              <p class="text-sm text-gray-500">حزمة المزامنة (.sync) هي المسار الحالي للمزامنة</p>
            </div>
          </div>
          <div class="space-y-2">
            <button
              on:click={importMonthlyReportSync}
              class="w-full btn-primary"
              disabled={!selectedUnit || !!importProgress}
              title="هذا هو مسار المزامنة الرسمي بين العقد"
            >
              استيراد حزمة المزامنة (.sync)
            </button>
          </div>
        </div>

        <!-- Import Stock Movements -->
        <div class="card">
          <div class="flex items-center gap-3 mb-4">
            <div class="w-12 h-12 bg-purple-50 rounded-lg flex items-center justify-center">
              <svg class="w-6 h-6 text-purple-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"/>
              </svg>
            </div>
            <div>
              <h3 class="font-semibold text-gray-800">حركات المخزون</h3>
              <p class="text-sm text-gray-500">استورد حركات المخزون من الوحدة</p>
            </div>
          </div>
          <div class="space-y-2">
            <button
              on:click={importStockMovementsPackageSync}
              class="w-full btn-primary bg-purple-600 hover:bg-purple-700 border-purple-700"
              disabled={!selectedUnit || !!importProgress}
              title="استيراد الحزمة المشفرة والموقعة رقمياً للمزامنة الآمنة"
            >
              استيراد حزمة حركات (.sync)
            </button>
          </div>
        </div>
      </div>

      <div class="mt-8 card bg-blue-50 border-blue-200">
        <h3 class="font-semibold text-blue-800 mb-2">كيفية استيراد تقارير الوحدة</h3>
        <ol class="text-sm text-blue-700 list-decimal list-inside space-y-1">
          <li><strong>من عقدة الوحدة:</strong> صدّر الملفات المطلوبة:
            <ul class="mr-4 mt-1 text-xs">
              <li>• تقرير يومي: صفحة "التقارير" → تصدير حزمة .sync</li>
              <li>• تقرير شهري: صفحة "التقارير" → تصدير حزمة .sync</li>
              <li>• <strong>حركات المخزون: صفحة "المخزون" → تصدير حزمة حركات (.sync)</strong></li>
            </ul>
          </li>
          <li><strong>من عقدة الولاية:</strong> اختر الوحدة المناسبة ثم استورد كل ملف</li>
        </ol>
      </div>
    {/if}
</Layout>
