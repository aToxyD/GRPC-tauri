<script lang="ts">
  import type { MonthlySummary } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';

  export let summary: MonthlySummary;

  const dispatch = createEventDispatcher();

  function close() {
    dispatch('close');
  }

  function viewDaily() {
    dispatch('viewDaily', { month: summary.month, year: summary.year });
  }
</script>

<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
  <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-2xl mx-4">
    <div class="p-6 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
      <div>
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">تفاصيل التقرير الشهري</h2>
        <p class="text-sm text-gray-500 dark:text-gray-400">{summary.month}/{summary.year}</p>
      </div>
      <button on:click={close} class="text-gray-400 hover:text-gray-600 dark:text-gray-400" aria-label="إغلاق">
        <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
        </svg>
      </button>
    </div>
    <div class="p-6">
      <div class="grid grid-cols-2 gap-4 mb-6">
        <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">عدد التقارير</p>
          <p class="text-lg font-bold text-civil-blue">{summary.report_count}</p>
        </div>
        <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">الموظفون</p>
          <p class="text-lg font-bold">{summary.total_personnel}</p>
        </div>
        <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">الضيوف</p>
          <p class="text-lg font-bold">{summary.total_guests}</p>
        </div>
        <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">إجمالي الوجبات</p>
          <p class="text-lg font-bold">{summary.total_meals}</p>
        </div>
        <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">القيمة الإجمالية</p>
          <p class="text-lg font-bold text-green-600">{summary.total_consumption_value.toFixed(2)} دج</p>
        </div>
        <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">المعدل/وجبة</p>
          <p class="text-lg font-bold">{summary.average_meal_rate.toFixed(2)} دج</p>
        </div>
      </div>
      <div class="flex justify-end gap-2">
        <button
          on:click={viewDaily}
          class="px-4 py-2 bg-civil-blue text-white rounded-lg hover:bg-civil-blue-dark"
        >
          عرض التقارير اليومية
        </button>
        <button
          on:click={close}
          class="px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg hover:bg-gray-50 dark:bg-gray-900"
        >
          إغلاق
        </button>
      </div>
    </div>
  </div>
</div>
