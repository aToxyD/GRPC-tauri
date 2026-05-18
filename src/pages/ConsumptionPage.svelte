<script lang="ts">
  import { onMount } from 'svelte';
  import { listProducts, createDailyReport, checkStockAvailability, getSettings } from '../lib/tauri';
  import type { Product, Settings, ConsumptionItemInput } from '../lib/types';
  import Layout from '../components/Layout.svelte';

  let products: Product[] = [];
  let settings: Settings | null = null;
  let loading = true;
  let error = '';
  let success = '';
  let submitting = false;

  // Form fields
  let date = new Date().toISOString().split('T')[0];
  let personnelCount = '';
  let guestCount = '';
  let consumptionItems: { product: Product; quantity: string; available: boolean; stock: number }[] = [];

  onMount(async () => {
    try {
      [products, settings] = await Promise.all([
        listProducts(),
        getSettings()
      ]);
      consumptionItems = products.map(p => ({ product: p, quantity: '', available: true, stock: 0 }));
      checkStocks();
    } catch (e) {
      error = 'خطأ في التحميل: ' + String(e);
    } finally {
      loading = false;
    }
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
      error = 'الرجاء إدخال التاريخ وعدد الموظفين';
      return;
    }

    const items: ConsumptionItemInput[] = consumptionItems
      .filter(ci => ci.quantity && parseFloat(ci.quantity) > 0)
      .map(ci => ({
        product_id: ci.product.id,
        quantity: parseFloat(ci.quantity)
      }));

    if (items.length === 0) {
      error = 'الرجاء إدخال استهلاك واحد على الأقل';
      return;
    }

    submitting = true;
    error = '';

    try {
      // Check stock availability before submitting
      const stockCheck = await checkStockAvailability(items);
      const insufficient = stockCheck.filter(s => !s.available);
      
      if (insufficient.length > 0) {
        error = `مخزون غير كافٍ لـ: ${insufficient.map(i => i.product_name).join(', ')}`;
        submitting = false;
        return;
      }

      const result = await createDailyReport({
        date,
        personnel_count: parseInt(personnelCount),
        guest_count: parseInt(guestCount) || 0,
        items
      }, settings?.unit_name || undefined);

      success = `تم تسجيل التقرير. التكلفة الإجمالية: ${result.report.total_meals_cost.toFixed(2)} دج، المعدل: ${result.report.actual_meal_rate.toFixed(2)} دج/وجبة`;
      
      // Reset form
      personnelCount = '';
      guestCount = '';
      consumptionItems = consumptionItems.map(ci => ({ ...ci, quantity: '' }));
      
      // Refresh stock status
      await checkStocks();
      
      setTimeout(() => success = '', 5000);
    } catch (e) {
      error = 'خطأ: ' + String(e);
    } finally {
      submitting = false;
    }
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

    {#if error}
      <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
        {error}
      </div>
    {/if}

    {#if success}
      <div class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm">
        {success}
      </div>
    {/if}

    {#if loading}
      <div class="flex items-center justify-center py-12">
        <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-civil-blue"></div>
      </div>
    {:else}
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Main Form -->
        <div class="lg:col-span-2 space-y-6">
          <div class="card">
            <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">المعلومات العامة</h2>
            <div class="grid grid-cols-3 gap-4">
              <div>
                <label for="date" class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1">التاريخ *</label>
                <input
                  id="date"
                  type="date"
                  class="input-field"
                  bind:value={date}
                />
              </div>
              <div>
                <label for="personnelCount" class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1">الموظفون *</label>
                <input
                  id="personnelCount"
                  type="number"
                  class="input-field"
                  placeholder="العدد"
                  bind:value={personnelCount}
                />
              </div>
              <div>
                <label for="guestCount" class="block text-sm font-medium text-gray-700 dark:text-gray-100 mb-1">الضيوف</label>
                <input
                  id="guestCount"
                  type="number"
                  class="input-field"
                  placeholder="العدد"
                  bind:value={guestCount}
                />
              </div>
            </div>
          </div>

          <div class="card">
            <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">المنتجات المستهلكة</h2>
            
            {#if products.length === 0}
              <div class="text-center py-8 text-gray-500 dark:text-gray-400">
                <p>لا يوجد منتجات متاحة. استورد قائمة الولاية أولاً.</p>
              </div>
            {:else}
              <div class="space-y-2 max-h-[400px] overflow-y-auto">
                {#each consumptionItems as item}
                  <div class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
                    <div class="flex-1">
                      <span class="font-medium block">{item.product.name}</span>
                      <span class="text-xs text-gray-500 dark:text-gray-400">
                        السعر: {item.product.base_price.toFixed(2)} دج | 
                        المخزون: <span class={item.stock < 10 ? 'text-red-600 font-semibold' : 'text-green-600'}>{item.stock.toFixed(2)}</span>
                      </span>
                    </div>
                    <input
                      type="number"
                      step="0.01"
                      class="input-field w-24"
                      placeholder="الكمية"
                      bind:value={item.quantity}
                      disabled={item.stock <= 0}
                    />
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <button
            on:click={submitReport}
            class="w-full btn-primary py-3 font-medium disabled:opacity-50"
            disabled={submitting || products.length === 0}
          >
            {#if submitting}
              <span class="flex items-center justify-center">
                <svg class="animate-spin -ml-1 mr-3 h-5 w-5 text-white" fill="none" viewBox="0 0 24 24">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                جاري التسجيل...
              </span>
            {:else}
              تسجيل التقرير
            {/if}
          </button>
        </div>

        <!-- Summary Panel -->
        <div class="lg:col-span-1">
          <div class="card sticky top-6">
            <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">الملخص</h2>
            
            <div class="space-y-4">
              <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                <span class="text-gray-600 dark:text-gray-400">الموظفون:</span>
                <span class="font-medium">{personnelCount || 0}</span>
              </div>
              <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                <span class="text-gray-600 dark:text-gray-400">الضيوف:</span>
                <span class="font-medium">{guestCount || 0}</span>
              </div>
              <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                <span class="text-gray-600 dark:text-gray-400">إجمالي الوجبات:</span>
                <span class="font-medium">{(parseInt(personnelCount) || 0) + (parseInt(guestCount) || 0)}</span>
              </div>
              <div class="flex justify-between items-center py-2 border-b border-gray-100 dark:border-gray-700">
                <span class="text-gray-600 dark:text-gray-400">القيمة المستهلكة:</span>
                <span class="font-bold text-civil-blue">{calculatedTotal.toFixed(2)} دج</span>
              </div>
              <div class="flex justify-between items-center py-2">
                <span class="text-gray-600 dark:text-gray-400">المعدل لكل وجبة:</span>
                <span class="font-bold text-civil-blue">{calculatedRate.toFixed(2)} دج</span>
              </div>
            </div>

            <div class="mt-6 p-3 bg-blue-50 dark:bg-blue-900/20 rounded-lg text-sm text-blue-700">
              <p class="font-medium mb-1">ملاحظة:</p>
              <p>يتم إجراء جميع الحسابات من قبل النظام الخلفي. البيانات المعروضة تأتي مباشرة من أوامر Tauri.</p>
            </div>
          </div>
        </div>
      </div>
    {/if}
</Layout>
