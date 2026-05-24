<script lang="ts">
  import AppCard from '../../lib/components/ui/AppCard.svelte';
  import BeneficiaryInputs from './BeneficiaryInputs.svelte';
  import MealProductsTable from './MealProductsTable.svelte';
  import MealSummaryCard from './MealSummaryCard.svelte';
  import type { MealType } from '../../lib/types';
  import type { ConsumptionProductRow, MealFormState, MealPreview } from './types';

  export let mealId: MealType;
  export let mealLabel: string;
  export let form: MealFormState;
  export let preview: MealPreview;
  export let productRows: ConsumptionProductRow[] = [];
  export let disabled = false;
  export let isPreview = true;
  export let fifoCosts: Record<string, { unitCost: number; lineTotal: number }> = {};
</script>

<AppCard>
  <h2 class="text-lg font-semibold text-civil-blue dark:text-blue-400 mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">
    {mealLabel}
  </h2>

  <h3 class="text-sm font-medium text-gray-600 dark:text-gray-400 mb-3">المستفيدون</h3>
  <BeneficiaryInputs idPrefix={mealId} bind:beneficiaries={form.beneficiaries} {disabled} />

  <h3 class="text-sm font-medium text-gray-600 dark:text-gray-400 mb-3 mt-6">المنتجات المستهلكة</h3>
  <MealProductsTable
    {mealId}
    bind:quantities={form.quantities}
    rows={productRows}
    {disabled}
    {fifoCosts}
    {isPreview}
  />

  <div class="mt-4">
    <MealSummaryCard {preview} {isPreview} />
  </div>
</AppCard>
