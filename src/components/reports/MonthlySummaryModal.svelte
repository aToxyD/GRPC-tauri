<script lang="ts">
  import type { MonthlySummary } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';
  import AppDialog from '../../lib/components/ui/AppDialog.svelte';
  import AppButton from '../../lib/components/ui/AppButton.svelte';

  export let summary: MonthlySummary;

  const dispatch = createEventDispatcher();

  function close() {
    dispatch('close');
  }

  function viewDaily() {
    dispatch('viewDaily', { month: summary.month, year: summary.year });
  }
</script>

<AppDialog
  open={true}
  title="تفاصيل التقرير الشهري"
  description="{summary.month}/{summary.year}"
  size="lg"
  on:close={close}
>
  <div class="grid grid-cols-2 gap-4">
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">عدد الأيام</p>
      <p class="text-lg font-bold text-civil-blue">{summary.report_count}</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">المستفيدون</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{summary.total_beneficiaries}</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">القيمة الإجمالية</p>
      <p class="text-lg font-bold text-green-600">{summary.total_consumption_value.toFixed(2)} دج</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">متوسط الفطور</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{summary.breakfast_average.toFixed(2)} دج</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">متوسط الغداء</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{summary.lunch_average.toFixed(2)} دج</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">متوسط العشاء</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{summary.dinner_average.toFixed(2)} دج</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg col-span-2">
      <p class="text-sm text-gray-500 dark:text-gray-400">المعدل اليومي الإجمالي</p>
      <p class="text-lg font-bold text-civil-blue">{summary.daily_average.toFixed(2)} دج</p>
    </div>
  </div>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={close}>إغلاق</AppButton>
    <AppButton variant="primary" on:click={viewDaily}>عرض التقارير اليومية</AppButton>
  </svelte:fragment>
</AppDialog>
