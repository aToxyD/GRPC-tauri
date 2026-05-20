<script lang="ts">
  import type { MealPreview } from './types';
  import { formatAmount } from './preview';

  export let preview: MealPreview;
  export let isPreview = true;
</script>

<div class="grid grid-cols-3 gap-3 text-sm bg-gray-50 dark:bg-gray-900/50 rounded-lg p-3 border border-gray-100 dark:border-gray-800">
  <div>
    <span class="text-gray-500 dark:text-gray-400 block">
      المستفيدون{#if isPreview} (معاينة){/if}
    </span>
    <span class="font-bold">{preview.totalBeneficiaries}</span>
  </div>
  <div>
    <span class="text-gray-500 dark:text-gray-400 block">
      التكلفة{#if isPreview} (معاينة){/if}
    </span>
    <span class="font-bold">{formatAmount(preview.totalCost)}</span>
  </div>
  <div>
    <span class="text-gray-500 dark:text-gray-400 block">
      المعدل{#if isPreview} (معاينة){/if}
    </span>
    <span class="font-bold">{formatAmount(preview.mealAverage)}/مستفيد</span>
  </div>
</div>
