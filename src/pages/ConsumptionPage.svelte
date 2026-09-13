<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatErrorMessage } from '../lib/errors';
  import {
    listProducts,
    createDailyReport,
    checkStockAvailability,
    getSettings,
    getDailyConsumption,
    previewDailyConsumptionFifo,
  } from '../lib/tauri';
  import type {
    Product,
    Settings,
    MealType,
    DailyConsumptionView,
    DailyFifoConsumptionPreview,
    MealSectionInput,
  } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';

  import DailySummaryPanel from '../components/consumption/DailySummaryPanel.svelte';
  import MealTabs from '../components/consumption/MealTabs.svelte';
  import MealSection from '../components/consumption/MealSection.svelte';
  import {
    hasAnyConsumption,
    mealItemsFromForm,
    mealPreviewFromFifo,
    parseBeneficiaryCounts,
    productFifoCostsForMeal,
  } from '../components/consumption/preview';
  import {
    emptyMealForms,
    MEAL_OPTIONS,
    type ConsumptionProductRow,
    type MealFormState,
  } from '../components/consumption/types';

  const scope = createRuntimeScope();
  const consumptionOp = createOperation({ scope });
  const loading = consumptionOp.loading;
  const error = consumptionOp.error;
  const { loading: submitting, guard } = createOperationGuard({ scope });

  // @category ProjectionState
  let products = $state<Product[]>([]);
  // @category ProjectionState
  let settings = $state<Settings | null>(null);
  // @category TransientState
  let success = $state('');
  // @category ProjectionState
  let dailyView = $state<DailyConsumptionView | null>(null);
  // @category ProjectionState
  let reportLocked = $state(false);
  // @category TransientState
  let date = $state(new Date().toISOString().split('T')[0]);
  // @category TransientState
  let activeMeal = $state<MealType>('breakfast');
  // @category TransientState
  let mealForms = $state<Record<MealType, MealFormState>>(emptyMealForms());
  // @category TransientState
  let consumptionItems = $state<ConsumptionProductRow[]>([]);
  // @category ProjectionState
  let fifoPreview = $state<DailyFifoConsumptionPreview | null>(null);
  // @category UiState
  let fifoPreviewLoading = $state(false);

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m), 5000);
  onDestroy(() => scope.dispose());

  // @category ProjectionState
  let displaySummary = $derived(
    dailyView
      ? dailyView.daily_summary
      : fifoPreview
        ? fifoPreview.daily_summary
        : {
            breakfast_beneficiaries: 0,
            lunch_beneficiaries: 0,
            dinner_beneficiaries: 0,
            breakfast_cost: 0,
            lunch_cost: 0,
            dinner_cost: 0,
            breakfast_average: 0,
            lunch_average: 0,
            dinner_average: 0,
            total_daily_beneficiaries: 0,
            total_daily_cost: 0,
            daily_average: 0,
          }
  );

  // @category ProjectionState
  let activeFifoMeal = $derived(
    fifoPreview?.meal_previews.find((m) => m.meal_type === activeMeal) ?? null
  );

  // @category ProjectionState
  let activeMealPreview = $derived(
    mealPreviewFromFifo(activeFifoMeal)
  );

  // @category UiState
  let activeMealFifoCosts = $derived(
    productFifoCostsForMeal(activeFifoMeal)
  );

  // @category UiState
  let activeMealLabel = $derived(
    MEAL_OPTIONS.find((m) => m.id === activeMeal)?.label ?? activeMeal
  );

  function applyDailyView(view: DailyConsumptionView) {
    dailyView = view;
    reportLocked = true;
    const next = emptyMealForms();
    for (const entry of view.meals) {
      const m = entry.meal;
      next[m.meal_type] = {
        beneficiaries: {
          staff24h: String(m.staff_24h_count),
          staff8h: String(m.staff_8h_count),
          reservation: String(m.reservation_count),
          mission: String(m.mission_count),
          guest: String(m.guest_count),
        },
        quantities: Object.fromEntries(entry.items.map((i) => [i.product_id, String(i.quantity)])),
      };
    }
    mealForms = next;
  }

  function resetForms() {
    dailyView = null;
    reportLocked = false;
    mealForms = emptyMealForms();
  }

  async function loadDailyData() {
    try {
      const view = await getDailyConsumption(date);
      if (view) {
        applyDailyView(view);
      } else {
        resetForms();
      }
    } catch {
      resetForms();
    }
  }

  async function checkStocks() {
    try {
      const stocks = await checkStockAvailability(
        products.map((p) => ({ product_id: p.id, quantity: 1 }))
      );
      consumptionItems = products.map((p) => {
        const stock = stocks.find((s) => s.product_id === p.id);
        return {
          product: p,
          available: stock?.available ?? true,
          stock: stock?.available_stock ?? 0,
        };
      });
    } catch {
      // stock check failed — keep UI usable
    }
  }

  onMount(async () => {
    await consumptionOp.run(async () => {
      [products, settings] = await Promise.all([listProducts(), getSettings()]);
      consumptionItems = products.map((p) => ({
        product: p,
        available: true,
        stock: 0,
      }));
      await checkStocks();
      await loadDailyData();
    });
  });

  async function handleDateChange() {
    await loadDailyData();
  }

  function buildMealInputs(): MealSectionInput[] {
    return MEAL_OPTIONS.map((meal) => {
      const form = mealForms[meal.id];
      const counts = parseBeneficiaryCounts(form.beneficiaries);
      return {
        meal_type: meal.id,
        ...counts,
        items: mealItemsFromForm(form),
      };
    });
  }

  async function refreshFifoPreview() {
    if (reportLocked || !date || !hasAnyConsumption(mealForms)) {
      fifoPreview = null;
      return;
    }
    fifoPreviewLoading = true;
    try {
      fifoPreview = await previewDailyConsumptionFifo({
        date,
        meals: buildMealInputs(),
      });
    } catch {
      fifoPreview = null;
    } finally {
      fifoPreviewLoading = false;
    }
  }

  $effect(() => {
    for (const meal of MEAL_OPTIONS) {
      mealForms[meal.id].quantities;
      parseBeneficiaryCounts(mealForms[meal.id].beneficiaries);
    }
    date;
    if (!reportLocked) {
      void refreshFifoPreview();
    }
  });

  async function submitDailyReport() {
    if (!date) {
      consumptionOp.error.set('الرجاء إدخال التاريخ');
      return;
    }
    if (!hasAnyConsumption(mealForms)) {
      consumptionOp.error.set('الرجاء إدخال بيانات استهلاك لوجبة واحدة على الأقل');
      return;
    }

    const allItems = MEAL_OPTIONS.flatMap((m) => mealItemsFromForm(mealForms[m.id]));
    if (allItems.length === 0) {
      consumptionOp.error.set('الرجاء إدخال استهلاك منتج واحد على الأقل');
      return;
    }

    await guard(async () => {
      const stockCheck = await checkStockAvailability(allItems);
      const insufficient = stockCheck.filter((s) => !s.available);
      if (insufficient.length > 0) {
        throw new Error(`مخزون غير كافٍ لـ: ${insufficient.map((i) => i.product_name).join(', ')}`);
      }

      const result = await createDailyReport(
        { date, meals: buildMealInputs() },
        settings?.unit_name || undefined
      );

      setSuccessWithTimeout(
        `تم تسجيل التقرير اليومي. التكلفة: ${result.report.total_daily_cost.toFixed(2)} دج، المعدل: ${result.report.total_daily_average.toFixed(2)} دج`
      );

      await loadDailyData();
      await checkStocks();
    });
  }
</script>

<Layout nodeType="UNIT" title="الاستهلاك اليومي" subtitle="تقرير يومي واحد — فطور · غداء · عشاء">
  <div dir="rtl">
    <AppPageHeader title="الاستهلاك اليومي" subtitle="تقرير استهلاك يومي واحد يتضمن الفطور والغداء والعشاء" />

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => consumptionOp.error.set(null)}>
          {formatErrorMessage($error)}
        </AppAlert>
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => (success = '')}>{success}</AppAlert>
      </div>
    {/if}

    {#if $loading}
      <AppLoadingState message="جاري التحميل..." />
    {:else}
      <div class="space-y-6">
        <AppCard>
          <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">
            المعلومات العامة
          </h2>
          <div class="max-w-xs">
            <AppInput
              id="date"
              label="التاريخ *"
              type="date"
              bind:value={date}
              on:change={handleDateChange}
              disabled={reportLocked}
            />
          </div>
          {#if reportLocked}
            <p class="mt-2 text-sm text-amber-600 dark:text-amber-400">
              تقرير هذا التاريخ مسجّل مسبقاً ولا يمكن تعديله من هذه الشاشة.
            </p>
          {/if}
        </AppCard>

        <DailySummaryPanel summary={displaySummary} isPreview={!dailyView} />

        <div class="space-y-4">
          <MealTabs bind:activeMeal {mealForms} />

          <div role="tabpanel" aria-label={activeMealLabel}>
            <MealSection
              mealId={activeMeal}
              mealLabel={activeMealLabel}
              bind:form={mealForms[activeMeal]}
              preview={activeMealPreview}
              productRows={consumptionItems}
              fifoCosts={activeMealFifoCosts}
              disabled={reportLocked}
              isPreview={!dailyView}
            />
            {#if fifoPreviewLoading && !dailyView}
              <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">جاري حساب تكلفة FIFO...</p>
            {/if}
          </div>
        </div>

        <AppButton
          variant="primary"
          fullWidth
          size="lg"
          loading={$submitting}
          disabled={$submitting || products.length === 0 || reportLocked}
          on:click={submitDailyReport}
        >
          {reportLocked ? 'التقرير اليومي مسجّل' : 'تسجيل التقرير اليومي'}
        </AppButton>
      </div>
    {/if}
  </div>
</Layout>
