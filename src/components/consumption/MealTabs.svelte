<script lang="ts">
  import type { MealType } from '../../lib/types';
  import type { MealFormState } from './types';
  import { MEAL_OPTIONS } from './types';
  import { mealHasActivity } from './preview';

  export let activeMeal: MealType;
  export let mealForms: Record<MealType, MealFormState>;
</script>

<div class="flex gap-2 border-b border-gray-200 dark:border-gray-700 pb-1" role="tablist" aria-label="وجبات اليوم">
  {#each MEAL_OPTIONS as meal (meal.id)}
    {@const hasData = mealHasActivity(mealForms[meal.id])}
    <button
      type="button"
      role="tab"
      aria-selected={activeMeal === meal.id}
      class="px-4 py-2 rounded-t-lg text-sm font-medium transition-colors
        {activeMeal === meal.id
        ? 'bg-civil-blue text-white'
        : 'bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700'}"
      on:click={() => (activeMeal = meal.id)}
    >
      {meal.label}
      {#if hasData}
        <span class="mr-1 opacity-80" aria-label="تحتوي على بيانات">●</span>
      {/if}
    </button>
  {/each}
</div>
