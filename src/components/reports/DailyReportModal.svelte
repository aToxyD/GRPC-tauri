<script lang="ts">
  import type { DailyReportResult, MealType, Product } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';
  import { unitLabel } from '../../lib/unitLabels';
  import AppDialog from '../../lib/components/ui/AppDialog.svelte';
  import AppTable from '../../lib/components/ui/AppTable.svelte';
  import AppButton from '../../lib/components/ui/AppButton.svelte';

  export let selectedReport: DailyReportResult;
  export let products: Product[] = [];

  const dispatch = createEventDispatcher();

  const MEAL_LABELS: Record<MealType, string> = {
    breakfast: 'فطور',
    lunch: 'غداء',
    dinner: 'عشاء',
  };

  // Presentation-only consumption unit of a product from the WILAYA catalog,
  // used to label the displayed quantities. Never a numeric conversion.
  // @category UiState
  function consumptionUnitFor(productId: string): string {
    const p = products.find((prod) => prod.id === productId);
    return p ? unitLabel(p.consumption_unit) : '—';
  }

  function close() {
    dispatch('close');
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString('fr-FR', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    });
  }
</script>

<AppDialog
  open={true}
  title="تفاصيل التقرير اليومي"
  description={formatDate(selectedReport.report.date)}
  size="xl"
  on:close={close}
>
  <div class="grid grid-cols-3 gap-4 mb-6 text-sm">
    <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-gray-500">المستفيدون</p>
      <p class="text-lg font-bold">{selectedReport.report.total_daily_beneficiaries}</p>
    </div>
    <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-gray-500">التكلفة</p>
      <p class="text-lg font-bold text-civil-blue">{selectedReport.report.total_daily_cost.toFixed(2)} دج</p>
    </div>
    <div class="text-center p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-gray-500">المعدل اليومي</p>
      <p class="text-lg font-bold text-civil-blue">{selectedReport.report.total_daily_average.toFixed(2)} دج</p>
    </div>
  </div>

  {#each selectedReport.meals as section}
    <div class="mb-6 border border-gray-200 dark:border-gray-700 rounded-xl overflow-hidden">
      <div class="px-4 py-2 bg-gray-50 dark:bg-gray-900 font-semibold text-gray-800 dark:text-gray-100">
        {MEAL_LABELS[section.meal.meal_type]}
      </div>
      <div class="p-4 grid grid-cols-2 md:grid-cols-4 gap-3 text-sm mb-4">
        <div><span class="text-gray-500">24س/48ر:</span> {section.meal.staff_24h_count}</div>
        <div><span class="text-gray-500">8 ساعات:</span> {section.meal.staff_8h_count}</div>
        <div><span class="text-gray-500">محجوزون:</span> {section.meal.reservation_count}</div>
        <div><span class="text-gray-500">مهمة:</span> {section.meal.mission_count}</div>
        <div><span class="text-gray-500">ضيوف:</span> {section.meal.guest_count}</div>
        <div><span class="text-gray-500">المستفيدون:</span> {section.meal.total_beneficiaries}</div>
        <div><span class="text-gray-500">التكلفة:</span> {section.meal.total_meal_cost.toFixed(2)} دج</div>
        <div><span class="text-gray-500">المعدل:</span> {section.meal.meal_average.toFixed(2)} دج</div>
      </div>
      <AppTable empty={section.items.length === 0}>
        <svelte:fragment slot="head">
          <th class="table-header">المنتج</th>
          <th class="table-header">وحدة الاستهلاك</th>
          <th class="table-header text-right">الكمية</th>
          <th class="table-header text-right">سعر الوحدة</th>
          <th class="table-header text-right">الإجمالي</th>
        </svelte:fragment>
        {#each section.items as item}
          <tr>
            <td class="table-cell">{item.product_name}</td>
            <td class="table-cell">{consumptionUnitFor(item.product_id)}</td>
            <td class="table-cell text-right">{item.quantity.toFixed(2)}</td>
            <td class="table-cell text-right">{item.unit_price.toFixed(2)} دج</td>
            <td class="table-cell text-right font-medium">{item.total_cost.toFixed(2)} دج</td>
          </tr>
        {/each}
      </AppTable>
    </div>
  {/each}

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={close}>إغلاق</AppButton>
  </svelte:fragment>
</AppDialog>
