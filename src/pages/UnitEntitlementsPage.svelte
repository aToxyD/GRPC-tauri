<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getSettings, listUnitContractEntitlements, listProducts } from '../lib/contracts';
  import type { Settings, UnitContractEntitlement, Product } from '../lib/types';
  import { unitLabel } from '../lib/unitLabels';
  import Layout from '../components/Layout.svelte';

  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppBadge from '../lib/components/ui/AppBadge.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';
  import AppTable from '../lib/components/ui/AppTable.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppProductSearch from '../lib/components/ui/AppProductSearch.svelte';

  import { createRuntimeScope } from '../lib/runtimeCleanup';
  import { createOperation } from '../lib/operationGuard';

  const scope = createRuntimeScope();
  onDestroy(() => scope.dispose());

  const entitlementsOp = createOperation({ scope });
  const { loading, error } = entitlementsOp;

  // @category ProjectionState
  let settings: Settings | null = null;
  // @category ProjectionState
  let entitlements: UnitContractEntitlement[] = [];
  // @category ProjectionState
  let products: Product[] = [];

  // Search is presentation/filtering only — it narrows the entitlement rows by
  // product name and never alters entitlement state or backend values.
  // @category UiState
  let searchText = '';

  // @category UiState
  let filteredEntitlements: UnitContractEntitlement[] = [];

  // @category UiState
  $: filteredEntitlements = !searchText.trim()
    ? entitlements
    : entitlements.filter((e) =>
        e.product_name.toLowerCase().includes(searchText.trim().toLowerCase())
      );

  onMount(async () => {
    await entitlementsOp.run(async () => {
      [entitlements, settings, products] = await Promise.all([
        listUnitContractEntitlements(),
        getSettings(),
        listProducts()
      ]);
    });
  });

  // Presentational only (A5/F3): the backend is the source of truth for
  // `effective_remaining`. These helpers only label rows for the user; they
  // never decide supplier selection nor re-derive any business quantity.
  // @category UiState
  $: currentYear = settings?.current_year ?? null;

  // @category UiState
  function isCurrentYear(row: UnitContractEntitlement): boolean {
    return currentYear !== null && row.fiscal_year === currentYear;
  }

  // @category UiState
  function isSatisfied(row: UnitContractEntitlement): boolean {
    return row.effective_remaining <= 0;
  }

  // @category UiState
  function obligationLabel(row: UnitContractEntitlement): string {
    if (isCurrentYear(row)) return 'استحقاق السنة الحالية';
    if (row.released_quantity > 0 && isSatisfied(row)) return 'التزام سابق - محرر/مغلق';
    if (isSatisfied(row)) return 'التزام سابق - مستوفى بالكامل';
    return 'التزام سابق قائم';
  }

  // @category UiState
  function obligationIntent(row: UnitContractEntitlement): 'success' | 'warning' | 'neutral' {
    if (isCurrentYear(row)) return 'neutral';
    if (isSatisfied(row)) return 'success';
    return 'warning';
  }

  // @category UiState
  function contractIntent(status: string): 'success' | 'warning' | 'danger' | 'neutral' {
    switch (status) {
      case 'ACTIVE': return 'success';
      case 'CANCELLED': return 'danger';
      case 'ENDED': return 'neutral';
      default: return 'neutral';
    }
  }

  // @category UiState
  $: hasReserved = entitlements.some(e => e.reserved_quantity > 0);
  // @category UiState
  $: subtitle = settings?.unit_name || 'مطعم الوحدة';
</script>

<Layout nodeType="UNIT" title="استحقاقات الكتالوج" {subtitle}>

  {#if $loading}
    <AppLoadingState message="جارٍ تحميل الاستحقاقات..." />
  {:else}
    <div class="mb-6">
      <AppAlert intent="info">
        البيانات المعروضة تعكس آخر كتالوج تم استيراده.
      </AppAlert>
    </div>
    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => error.set(null)}>
          {$error}
        </AppAlert>
      </div>
    {/if}

    <AppCard padding="none">
      <div class="p-4 border-b border-gray-100 dark:border-gray-700">
        <h2 class="text-xl font-semibold text-gray-800 dark:text-gray-100">استحقاقات العقود - قراءة فقط</h2>
        <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
          الكميات المتبقية التقديرية محتسبة من جانب النظام (المصدر الوحيد للقيمة).
        </p>
      </div>

      <div class="p-4 border-b border-gray-100 dark:border-gray-700 flex items-center justify-end">
        <AppProductSearch bind:search={searchText} />
      </div>

      <AppTable empty={filteredEntitlements.length === 0}>
        <svelte:fragment slot="empty">
          {#if entitlements.length > 0}
            <AppEmptyState
              title="لا توجد نتائج مطابقة"
              description="لم يتم العثور على استحقاق مطابق لبحثك."
              icon="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
            />
          {:else}
            <AppEmptyState
              title="لا توجد استحقاقات معروضة"
              description="تُعرض هنا استحقاقات العقود المستوردة عبر كتالوج العقود"
              icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          {/if}
        </svelte:fragment>

        <svelte:fragment slot="head">
          <th class="table-header">المنتج</th>
          <th class="table-header">وحدة الشراء</th>
          <th class="table-header">المورد</th>
          <th class="table-header">السنة المالية</th>
          <th class="table-header text-right">التعاقد</th>
          <th class="table-header text-right">المستلم</th>
          <th class="table-header text-right">المسموح به</th>
          {#if hasReserved}
            <th class="table-header text-right">المحجوز</th>
          {/if}
          <th class="table-header text-right">المتبقي التقديري</th>
          <th class="table-header text-right">السعر المتفق</th>
          <th class="table-header">حالة العقد</th>
          <th class="table-header">النافذة</th>
        </svelte:fragment>

        {#each filteredEntitlements as e}
          {@const purchaseUnit = products.find((pr) => pr.id === e.product_id)}
          <tr class="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
            <td class="table-cell font-medium">{e.product_name}</td>
            <td class="table-cell">{purchaseUnit ? unitLabel(purchaseUnit.purchase_unit) : '—'}</td>
            <td class="table-cell">{e.supplier_name}</td>
            <td class="table-cell">{e.fiscal_year}</td>
            <td class="table-cell text-right">{e.contracted_quantity.toFixed(2)}</td>
            <td class="table-cell text-right">{e.fulfilled_quantity.toFixed(2)}</td>
            <td class="table-cell text-right">{e.released_quantity.toFixed(2)}</td>
            {#if hasReserved}
              <td class="table-cell text-right">{e.reserved_quantity > 0 ? e.reserved_quantity.toFixed(2) : '—'}</td>
            {/if}
            <td class="table-cell text-right font-semibold {e.effective_remaining > 0 ? 'text-emerald-600 dark:text-emerald-400' : 'text-gray-500 dark:text-gray-400'}">
              {e.effective_remaining.toFixed(2)}
            </td>
            <td class="table-cell text-right">{e.price_ttc != null ? `${e.price_ttc.toFixed(2)} DA` : '—'}</td>
            <td class="table-cell">
              <AppBadge intent={contractIntent(e.contract_status)} size="sm">{e.contract_status}</AppBadge>
            </td>
            <td class="table-cell">
              <AppBadge intent={obligationIntent(e)} size="sm">{obligationLabel(e)}</AppBadge>
            </td>
          </tr>
        {/each}
      </AppTable>
    </AppCard>
  {/if}
</Layout>
