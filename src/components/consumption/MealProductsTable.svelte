<script lang="ts">
  import AppInput from '../../lib/components/ui/AppInput.svelte';
  import AppTable from '../../lib/components/ui/AppTable.svelte';
  import AppEmptyState from '../../lib/components/ui/AppEmptyState.svelte';
  import type { MealType, ProductFifoPreview } from '../../lib/types';
  import type { ConsumptionProductRow } from './types';

  let {
    mealId,
    quantities = $bindable<Record<string, string>>({}),
    rows = [],
    disabled = false,
    fifoCosts = [],
    isPreview = true,
  }: {
    mealId: MealType;
    quantities?: Record<string, string>;
    rows?: ConsumptionProductRow[];
    disabled?: boolean;
    fifoCosts?: ProductFifoPreview[];
    isPreview?: boolean;
  } = $props();

  function fifoFor(productId: string): ProductFifoPreview | undefined {
    return fifoCosts.find((f) => f.product_id === productId);
  }

  function unitCostLabel(productId: string): string {
    const f = fifoFor(productId);
    if (!f || f.predicted_fifo_cost <= 0) return '—';
    return `${f.unit_cost.toFixed(2)} دج`;
  }

  function lineTotalLabel(productId: string): string {
    const f = fifoFor(productId);
    if (!f || f.predicted_fifo_cost <= 0) return '—';
    return `${f.predicted_fifo_cost.toFixed(2)} دج`;
  }
</script>

{#if rows.length === 0}
  <AppEmptyState
    title="لا يوجد منتجات"
    description="استورد قائمة منتجات الولاية أولاً"
    icon="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
  />
{:else}
  <div class="max-h-[360px] overflow-y-auto">
    <AppTable caption="منتجات الوجبة">
      <svelte:fragment slot="head">
        <th class="table-header">المنتج</th>
        <th class="table-header text-right w-28">الكمية</th>
        <th class="table-header text-right w-36">
          {isPreview ? 'تكلفة الوحدة (FIFO)' : 'تكلفة الوحدة'}
        </th>
        <th class="table-header text-right w-32">الإجمالي (FIFO)</th>
      </svelte:fragment>

      {#each rows as row (row.product.id)}
        {@const qty = quantities[row.product.id] ?? ''}
        <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
          <td class="table-cell">
            <span class="font-medium block text-gray-800 dark:text-gray-200">{row.product.name}</span>
            <span class="text-xs text-gray-500 dark:text-gray-400">
              المخزون:
              <span class={row.stock < 10 ? 'text-red-600 font-semibold' : 'text-green-600 dark:text-green-400'}>
                {row.stock.toFixed(2)}
              </span>
            </span>
          </td>
          <td class="table-cell text-right">
            <AppInput
              id="qty-{mealId}-{row.product.id}"
              label=""
              type="number"
              placeholder="0"
              bind:value={quantities[row.product.id]}
              disabled={disabled || (row.stock <= 0 && !qty)}
              class="text-right"
            />
          </td>
          <td class="table-cell text-right text-gray-600 dark:text-gray-400 tabular-nums">
            {unitCostLabel(row.product.id)}
          </td>
          <td class="table-cell text-right font-medium tabular-nums">
            {lineTotalLabel(row.product.id)}
          </td>
        </tr>
      {/each}
    </AppTable>
  </div>
{/if}
