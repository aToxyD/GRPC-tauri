<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { formatErrorMessage } from '../lib/errors';
  import { listProducts, createDailyReport, checkStockAvailability, getSettings } from '../lib/tauri';
  import type { Product, Settings, ConsumptionItemInput } from '../lib/types';
  import Layout from '../components/Layout.svelte';
  import { createOperation, createOperationGuard } from '../lib/operationGuard';
  import { createRuntimeScope, createTransientMessage } from '../lib/runtimeCleanup';

  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppPageHeader from '../lib/components/ui/AppPageHeader.svelte';
  import AppLoadingState from '../lib/components/ui/AppLoadingState.svelte';
  import AppEmptyState from '../lib/components/ui/AppEmptyState.svelte';

  const scope = createRuntimeScope();
  const consumptionOp = createOperation({ scope });
  const loading = consumptionOp.loading;
  const error = consumptionOp.error;

  const { loading: submitting, guard } = createOperationGuard({ scope });

  let products: Product[] = [];
  let settings: Settings | null = null;
  let success = '';

  const setSuccessWithTimeout = createTransientMessage(scope, (m) => (success = m), 5000);
  onDestroy(() => scope.dispose());

  // Form fields
  let date = new Date().toISOString().split('T')[0];
  let personnelCount = '';
  let guestCount = '';
  let consumptionItems: { product: Product; quantity: string; available: boolean; stock: number }[] = [];

  onMount(async () => {
    await consumptionOp.run(async () => {
      [products, settings] = await Promise.all([
        listProducts(),
        getSettings()
      ]);
      consumptionItems = products.map(p => ({ product: p, quantity: '', available: true, stock: 0 }));
      await checkStocks();
    });
  });

  async function checkStocks() {
    try {
      const stocks = await checkStockAvailability(
        products.map(p => ({ product_id: p.id, quantity: 1 }))
      );
      consumptionItems = consumptionItems.map(item => {
        const stock = stocks.find(s => s.product_id === item.product.id);
        return {
          ...item,
          available: stock?.available ?? true,
          stock: stock?.available_stock ?? 0
        };
      });
    } catch (e) {
      // Error checking stocks
    }
  }

  async function submitReport() {
    if (!date || !personnelCount) {
      consumptionOp.error.set('الرجاء إدخال التاريخ وعدد الموظفين');
      return;
    }

    const items: ConsumptionItemInput[] = consumptionItems
      .filter(ci => ci.quantity && parseFloat(ci.quantity) > 0)
      .map(ci => ({
        product_id: ci.product.id,
        quantity: parseFloat(ci.quantity)
      }));

    if (items.length === 0) {
      consumptionOp.error.set('الرجاء إدخال استهلاك واحد على الأقل');
      return;
    }

    await guard(async () => {
      // Check stock availability before submitting
      const stockCheck = await checkStockAvailability(items);
      const insufficient = stockCheck.filter(s => !s.available);
      
      if (insufficient.length > 0) {
        throw new Error(`مخزون غير كافٍ لـ: ${insufficient.map(i => i.product_name).join(', ')}`);
      }

      const result = await createDailyReport({
        date,
        personnel_count: parseInt(personnelCount),
        guest_count: parseInt(guestCount) || 0,
        items
      }, settings?.unit_name || undefined);

      setSuccessWithTimeout(`تم تسجيل التقرير. التكلفة الإجمالية: ${result.report.total_meals_cost.toFixed(2)} دج، المعدل: ${result.report.actual_meal_rate.toFixed(2)} دج/وجبة`);
      
      // Reset form
      personnelCount = '';
      guestCount = '';
      consumptionItems = consumptionItems.map(ci => ({ ...ci, quantity: '' }));
      
      // Refresh stock status
      await checkStocks();
    });
  }

  // Calculate totals
  let calculatedTotal = 0;
  let calculatedRate = 0;

  async function updateCalculations() {
    const items = consumptionItems
      .filter(ci => ci.quantity && parseFloat(ci.quantity) > 0)
      .map(ci => [parseFloat(ci.quantity), ci.product.base_price] as [number, number]);
    
    if (items.length > 0) {
      calculatedTotal = items.reduce((sum, [qty, price]) => sum + qty * price, 0);
      const totalMeals = (parseInt(personnelCount) || 0) + (parseInt(guestCount) || 0);
      calculatedRate = totalMeals > 0 ? calculatedTotal / totalMeals : 0;
    } else {
      calculatedTotal = 0;
      calculatedRate = 0;
    }
  }

  $: {
    consumptionItems;
    personnelCount;
    guestCount;
    updateCalculations();
  }
</script>

<Layout nodeType="UNIT" title="الاستهلاك اليومي" subtitle="تسجيل استهلاك الوجبات">
  <div dir="rtl">
    <AppPageHeader title="الاستهلاك اليومي" subtitle="تسجيل استهلاك الوجبات" />

    {#if $error}
      <div class="mb-4">
        <AppAlert intent="danger" dismissible on:dismiss={() => consumptionOp.error.set(null)}>{$error}</AppAlert>
      </div>
    {/if}

    {#if success}
      <div class="mb-4">
        <AppAlert intent="success" dismissible on:dismiss={() => success = ''}>{success}</AppAlert>
      </div>
    {/if}

    {#if $loading}
      <AppLoadingState message="جاري التحميل..." />
    {:else}
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Main Form -->
        <div class="lg:col-span-2 space-y-6">
          <AppCard>
            <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">المعلومات العامة</h2>
            <div class="grid grid-cols-3 gap-4">
              <AppInput
                id="date"
                label="التاريخ *"
                type="date"
                bind:value={date}
              />
              <AppInput
                id="personnelCount"
                label="الموظفون *"
                type="number"
                placeholder="العدد"
                bind:value={personnelCount}
              />
              <AppInput
                id="guestCount"
                label="الضيوف"
                type="number"
                placeholder="العدد"
                bind:value={guestCount}
              />
            </div>
          </AppCard>

          <AppCard>
            <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">المنتجات المستهلكة</h2>
            
            {#if products.length === 0}
              <AppEmptyState
                title="لا يوجد منتجات"
                description="استورد قائمة منتجات الولاية أولاً للتمكن من تسجيل الاستهلاك"
                icon="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"
              />
            {:else}
              <div class="space-y-2 max-h-[400px] overflow-y-auto pr-2">
                {#each consumptionItems as item}
                  <div class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-900 rounded-lg border border-gray-200 dark:border-gray-800">
                    <div class="flex-1">
                      <span class="font-medium block text-gray-800 dark:text-gray-200">{item.product.name}</span>
                      <span class="text-xs text-gray-500 dark:text-gray-400">
                        السعر: {item.product.base_price.toFixed(2)} دج | 
                        المخزون: <span class={item.stock < 10 ? 'text-red-600 font-semibold' : 'text-green-600 dark:text-green-400'}>{item.stock.toFixed(2)}</span>
                      </span>
                    </div>
                    <div class="w-24">
                      <AppInput
                        id="qty-{item.product.id}"
                        label=""
                        type="number"
                        placeholder="الكمية"
                        bind:value={item.quantity}
                        disabled={item.stock <= 0}
                      />
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </AppCard>

          <AppButton
            variant="primary"
            fullWidth
            size="lg"
            loading={$submitting}
            disabled={$submitting || products.length === 0}
            on:click={submitReport}
          >
            تسجيل التقرير
          </AppButton>
        </div>

        <!-- Summary Panel -->
        <div class="lg:col-span-1">
          <div class="sticky top-6">
            <AppCard>
              <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4 border-b border-gray-100 dark:border-gray-700 pb-2">الملخص</h2>
              
              <div class="space-y-4">
                <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                  <span class="text-gray-600 dark:text-gray-400">الموظفون:</span>
                  <span class="font-medium text-gray-800 dark:text-gray-200">{personnelCount || 0}</span>
                </div>
                <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                  <span class="text-gray-600 dark:text-gray-400">الضيوف:</span>
                  <span class="font-medium text-gray-800 dark:text-gray-200">{guestCount || 0}</span>
                </div>
                <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                  <span class="text-gray-600 dark:text-gray-400">إجمالي الوجبات:</span>
                  <span class="font-medium text-gray-800 dark:text-gray-200">{(parseInt(personnelCount) || 0) + (parseInt(guestCount) || 0)}</span>
                </div>
                <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                  <span class="text-gray-600 dark:text-gray-400">القيمة المستهلكة:</span>
                  <span class="font-bold text-civil-blue dark:text-blue-400">{calculatedTotal.toFixed(2)} دج</span>
                </div>
                <div class="flex justify-between items-center py-2">
                  <span class="text-gray-600 dark:text-gray-400">المعدل لكل وجبة:</span>
                  <span class="font-bold text-civil-blue dark:text-blue-400">{calculatedRate.toFixed(2)} دج</span>
                </div>
              </div>

              <div class="mt-6">
                <AppAlert intent="info" title="ملاحظة">
                  <span class="text-sm">يتم إجراء جميع الحسابات من قبل النظام الخلفي. البيانات المعروضة تأتي مباشرة من أوامر Tauri.</span>
                </AppAlert>
              </div>
            </AppCard>
          </div>
        </div>
      </div>
    {/if}
  </div>
</Layout>
