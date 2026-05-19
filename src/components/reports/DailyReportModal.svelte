<script lang="ts">
  import type { DailyReportResult } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';
  import AppDialog from '../../lib/components/ui/AppDialog.svelte';
  import AppTable from '../../lib/components/ui/AppTable.svelte';
  import AppButton from '../../lib/components/ui/AppButton.svelte';

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

<AppDialog
  open={true}
  title="تفاصيل التقرير"
  description={formatDate(selectedReport.report.date)}
  size="xl"
  on:close={close}
>
  <div class="grid grid-cols-4 gap-4 mb-6">
    <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">الموظفون</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{selectedReport.report.personnel_count}</p>
    </div>
    <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">الضيوف</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{selectedReport.report.guest_count}</p>
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
  
  <AppTable empty={selectedReport.items.length === 0}>
    <svelte:fragment slot="head">
      <th class="table-header">المنتج</th>
      <th class="table-header text-right">الكمية</th>
      <th class="table-header text-right">سعر الوحدة</th>
      <th class="table-header text-right">الإجمالي</th>
    </svelte:fragment>

    {#each selectedReport.items as item}
      <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
        <td class="table-cell">{item.product_name}</td>
        <td class="table-cell text-right">{item.quantity.toFixed(2)}</td>
        <td class="table-cell text-right">{item.unit_price.toFixed(2)} دج</td>
        <td class="table-cell text-right font-medium">{item.total_cost.toFixed(2)} دج</td>
      </tr>
    {/each}
  </AppTable>

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={close}>إغلاق</AppButton>
  </svelte:fragment>
</AppDialog>
