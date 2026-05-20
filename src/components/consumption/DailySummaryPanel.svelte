<script lang="ts">
  import AppCard from '../../lib/components/ui/AppCard.svelte';
  import AppAlert from '../../lib/components/ui/AppAlert.svelte';
  import type { DailyConsumptionSummary } from '../../lib/types';
  import { formatAmount } from './preview';

  export let summary: DailyConsumptionSummary;
  export let isPreview = true;
</script>

<AppCard>
  <div class="flex items-center justify-between mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">
    <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100">الملخص اليومي</h2>
    {#if isPreview}
      <span class="text-xs font-medium text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-900/30 px-2 py-1 rounded-full">
        معاينة مباشرة
      </span>
    {:else}
      <span class="text-xs font-medium text-green-600 dark:text-green-400 bg-green-50 dark:bg-green-900/30 px-2 py-1 rounded-full">
        محفوظ
      </span>
    {/if}
  </div>

  <div class="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm mb-4">
    <div class="rounded-lg border border-gray-100 dark:border-gray-800 p-3">
      <p class="font-medium text-gray-700 dark:text-gray-300 mb-2">الفطور</p>
      <div class="space-y-1 text-gray-600 dark:text-gray-400">
        <div class="flex justify-between"><span>المستفيدون</span><span>{summary.breakfast_beneficiaries}</span></div>
        <div class="flex justify-between"><span>التكلفة</span><span>{formatAmount(summary.breakfast_cost)}</span></div>
        <div class="flex justify-between"><span>المعدل</span><span>{formatAmount(summary.breakfast_average)}</span></div>
      </div>
    </div>
    <div class="rounded-lg border border-gray-100 dark:border-gray-800 p-3">
      <p class="font-medium text-gray-700 dark:text-gray-300 mb-2">الغداء</p>
      <div class="space-y-1 text-gray-600 dark:text-gray-400">
        <div class="flex justify-between"><span>المستفيدون</span><span>{summary.lunch_beneficiaries}</span></div>
        <div class="flex justify-between"><span>التكلفة</span><span>{formatAmount(summary.lunch_cost)}</span></div>
        <div class="flex justify-between"><span>المعدل</span><span>{formatAmount(summary.lunch_average)}</span></div>
      </div>
    </div>
    <div class="rounded-lg border border-gray-100 dark:border-gray-800 p-3">
      <p class="font-medium text-gray-700 dark:text-gray-300 mb-2">العشاء</p>
      <div class="space-y-1 text-gray-600 dark:text-gray-400">
        <div class="flex justify-between"><span>المستفيدون</span><span>{summary.dinner_beneficiaries}</span></div>
        <div class="flex justify-between"><span>التكلفة</span><span>{formatAmount(summary.dinner_cost)}</span></div>
        <div class="flex justify-between"><span>المعدل</span><span>{formatAmount(summary.dinner_average)}</span></div>
      </div>
    </div>
  </div>

  <div class="grid grid-cols-1 sm:grid-cols-3 gap-3 pt-3 border-t border-gray-200 dark:border-gray-700">
    <div class="text-center p-3 bg-civil-blue/5 dark:bg-blue-900/20 rounded-lg">
      <p class="text-xs text-gray-500 dark:text-gray-400">إجمالي المستفيدين</p>
      <p class="text-lg font-bold text-civil-blue dark:text-blue-400">{summary.total_daily_beneficiaries}</p>
    </div>
    <div class="text-center p-3 bg-civil-blue/5 dark:bg-blue-900/20 rounded-lg">
      <p class="text-xs text-gray-500 dark:text-gray-400">التكلفة اليومية</p>
      <p class="text-lg font-bold text-civil-blue dark:text-blue-400">{formatAmount(summary.total_daily_cost)}</p>
    </div>
    <div class="text-center p-3 bg-civil-blue/5 dark:bg-blue-900/20 rounded-lg">
      <p class="text-xs text-gray-500 dark:text-gray-400">المعدل اليومي</p>
      <p class="text-lg font-bold text-civil-blue dark:text-blue-400">{formatAmount(summary.daily_average)}</p>
    </div>
  </div>

  {#if isPreview}
    <div class="mt-4">
      <AppAlert intent="info" title="ملاحظة">
        <span class="text-sm">
          المعاينة في الواجهة للعرض فقط. عند الحفظ يعيد النظام الخلفي حساب جميع المجاميع والمعدلات وهي المصدر الرسمي.
        </span>
      </AppAlert>
    </div>
  {/if}
</AppCard>
