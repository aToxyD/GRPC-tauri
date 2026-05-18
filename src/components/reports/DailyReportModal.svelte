<script lang="ts">
  import type { DailyReportResult } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';

  export let selectedReport: DailyReportResult;

  const dispatch = createEventDispatcher();

  function close() {
    dispatch('close');
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString('fr-FR', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric'
    });
  }
</script>

<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
  <div class="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-3xl mx-4 max-h-[90vh] overflow-hidden">
    <div class="p-6 border-b border-gray-100 dark:border-gray-700 flex items-center justify-between">
      <div>
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">تفاصيل التقرير</h2>
        <p class="text-sm text-gray-500 dark:text-gray-400">{formatDate(selectedReport.report.date)}</p>
      </div>
      <button on:click={close} class="text-gray-400 hover:text-gray-600 dark:text-gray-400" aria-label="إغلاق">
        <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
        </svg>
      </button>
    </div>
    <div class="p-6 overflow-y-auto max-h-[60vh]">
      <div class="grid grid-cols-4 gap-4 mb-6">
        <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">الموظفون</p>
          <p class="text-lg font-bold">{selectedReport.report.personnel_count}</p>
        </div>
        <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">الضيوف</p>
          <p class="text-lg font-bold">{selectedReport.report.guest_count}</p>
        </div>
        <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">التكلفة الإجمالية</p>
          <p class="text-lg font-bold text-civil-blue">{selectedReport.report.total_meals_cost.toFixed(2)} دج</p>
        </div>
        <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
          <p class="text-sm text-gray-500 dark:text-gray-400">المعدل/وجبة</p>
          <p class="text-lg font-bold text-civil-blue">{selectedReport.report.actual_meal_rate.toFixed(2)} دج</p>
        </div>
      </div>
      <h3 class="font-semibold text-gray-800 dark:text-gray-100 mb-3">المنتجات المستهلكة</h3>
      <table class="w-full">
        <thead>
          <tr>
            <th class="table-header">المنتج</th>
            <th class="table-header text-right">الكمية</th>
            <th class="table-header text-right">سعر الوحدة</th>
            <th class="table-header text-right">الإجمالي</th>
          </tr>
        </thead>
        <tbody>
          {#each selectedReport.items as item}
            <tr>
              <td class="table-cell">{item.product_name}</td>
              <td class="table-cell text-right">{item.quantity.toFixed(2)}</td>
              <td class="table-cell text-right">{item.unit_price.toFixed(2)} دج</td>
              <td class="table-cell text-right font-medium">{item.total_cost.toFixed(2)} دج</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</div>
