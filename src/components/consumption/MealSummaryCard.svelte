<script lang="ts">
  import { formatAmount } from './preview';

  // [arch:allow-fe146] see ADR-0054 — Reason: type definition mirroring backend projection shape; Date: 2026-08-30; Owner: governance-team
  type SummaryCardPreview = { totalBeneficiaries: number; totalCost: number; mealAverage: number };

  export let preview: SummaryCardPreview | null = null;
  export let isPreview = true;
</script>

<div class="grid grid-cols-3 gap-3 text-sm bg-gray-50 dark:bg-gray-900/50 rounded-lg p-3 border border-gray-100 dark:border-gray-800">
  <div>
    <span class="text-gray-500 dark:text-gray-400 block">
      المستفيدون{#if isPreview} (معاينة){/if}
    </span>
    <span class="font-bold">{preview?.totalBeneficiaries ?? 0}</span>
  </div>
  <div>
    <span class="text-gray-500 dark:text-gray-400 block">
      {isPreview ? 'تكلفة FIFO المتوقعة' : 'التكلفة'}
    </span>
    <span class="font-bold">{formatAmount(preview?.totalCost ?? 0)}</span>
  </div>
  <div>
    <span class="text-gray-500 dark:text-gray-400 block">
      المعدل{#if isPreview} (معاينة){/if}
    </span>
    <span class="font-bold">{formatAmount(preview?.mealAverage ?? 0)}/مستفيد</span>
  </div>
</div>
