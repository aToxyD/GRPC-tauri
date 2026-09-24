<script lang="ts">
  import AppCard from '../../lib/components/ui/AppCard.svelte';
  import AppEmptyState from '../../lib/components/ui/AppEmptyState.svelte';
  import AppProductSearch from '../../lib/components/ui/AppProductSearch.svelte';
  import BeneficiaryInputs from './BeneficiaryInputs.svelte';
  import MealProductsTable from './MealProductsTable.svelte';
  import MealSummaryCard from './MealSummaryCard.svelte';
  import type { MealType, ProductFifoPreview } from '../../lib/types';
  import type { ConsumptionProductRow, MealFormState } from './types';

  // [arch:allow-fe146] see ADR-0054 — Reason: type definition mirroring backend projection shape; Date: 2026-08-30; Owner: governance-team
  type ActiveMealSummary = { totalBeneficiaries: number; totalCost: number; mealAverage: number };

  let {
    mealId,
    mealLabel,
    form = $bindable(),
    preview = null,
    productRows = [],
    disabled = false,
    isPreview = true,
    fifoCosts = {} as Record<string, { unitCost: number; lineTotal: number }>,
  }: {
    mealId: MealType;
    mealLabel: string;
    form: MealFormState;
    preview: ActiveMealSummary | null;
    productRows: ConsumptionProductRow[];
    disabled: boolean;
    isPreview: boolean;
    fifoCosts: Record<string, { unitCost: number; lineTotal: number }>;
  } = $props();

  // Search is presentation/filtering only (F3). It never alters quantities,
  // assignments, or backend state.
  // @category UiState
  let searchText = $state('');

  // Products already configured/assigned to this meal: for a locked (saved)
  // report the assigned set is the recorded meal items; for an editable meal
  // the full product list remains available so new items can be configured.
  // @category UiState
  let assignedProductIds = $derived(
    disabled
      ? Object.keys(form.quantities).filter(
          (id) => (parseFloat(form.quantities[id] ?? '') || 0) > 0
        )
      : productRows.map((row) => row.product.id)
  );

  // @category UiState
  let searchTerm = $derived(searchText.trim().toLowerCase());

  // Search narrows the base collection (assigned products for the meal); it
  // never broadens it by exposing unassigned catalog products.
  // @category UiState
  let visibleRows = $derived(
    productRows.filter(
      (row) =>
        assignedProductIds.includes(row.product.id) &&
        (!searchTerm || row.product.name.toLowerCase().includes(searchTerm))
    )
  );
</script>

<AppCard>
  <h2 class="text-lg font-semibold text-civil-blue dark:text-blue-400 mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">
    {mealLabel}
  </h2>

  <h3 class="text-sm font-medium text-gray-600 dark:text-gray-400 mb-3">المستفيدون</h3>
  <BeneficiaryInputs idPrefix={mealId} bind:beneficiaries={form.beneficiaries} {disabled} />

  <div class="flex items-center justify-between gap-3 mb-3 mt-6">
    <h3 class="text-sm font-medium text-gray-600 dark:text-gray-400">المنتجات المستهلكة</h3>
    <AppProductSearch bind:search={searchText} />
  </div>

  {#if visibleRows.length === 0 && searchText.trim() !== ''}
    <AppEmptyState
      title="لا توجد نتائج مطابقة"
      description="لم يتم العثور على منتج مطابق لبحثك ضمن منتجات هذه الوجبة."
      icon="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
    />
  {:else}
    <MealProductsTable
      {mealId}
      bind:quantities={form.quantities}
      rows={visibleRows}
      {disabled}
      fifoCosts={fifoCosts}
      {isPreview}
    />
  {/if}

  <div class="mt-4">
    <MealSummaryCard {preview} {isPreview} />
  </div>
</AppCard>
