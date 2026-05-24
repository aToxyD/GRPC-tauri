<script lang="ts">
  import AppInput from '../../lib/components/ui/AppInput.svelte';
  import AppTable from '../../lib/components/ui/AppTable.svelte';
  import AppEmptyState from '../../lib/components/ui/AppEmptyState.svelte';
  import type { MealType } from '../../lib/types';
  import type { ConsumptionProductRow } from './types';

  export let mealId: MealType;
  export let quantities: Record<string, string>;
  export let rows: ConsumptionProductRow[] = [];
  export let disabled = false;
  /** FIFO unit cost and line total per product (from backend preview). */
  export let fifoCosts: Record<string, { unitCost: number; lineTotal: number }> = {};
  export let isPreview = true;

  function lineTotal(productId: string): number {
    const fifo = fifoCosts[productId];
    if (fifo) return fifo.lineTotal;
    const qty = parseFloat(quantities[productId] ?? '') || 0;
    return qty > 0 ? 0 : 0;
  }

  function unitCostDisplay(productId: string): string {
    const fifo = fifoCosts[productId];
    if (fifo && fifo.unitCost > 0) return fifo.unitCost.toFixed(2);
    return '—';
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
        {@const total = lineTotal(row.product.id)}
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
            {unitCostDisplay(row.product.id)}{#if fifoCosts[row.product.id]} دج{/if}
          </td>
          <td class="table-cell text-right font-medium tabular-nums">
            {total > 0 ? `${total.toFixed(2)} دج` : '—'}
          </td>
        </tr>
      {/each}
    </AppTable>
  </div>
{/if}
