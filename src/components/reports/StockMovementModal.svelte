<script lang="ts">
  import type { StockMovement } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';
  import AppDialog from '../../lib/components/ui/AppDialog.svelte';
  import AppButton from '../../lib/components/ui/AppButton.svelte';
  import AppBadge from '../../lib/components/ui/AppBadge.svelte';

  export let movement: StockMovement;

  const dispatch = createEventDispatcher();

  function close() {
    dispatch('close');
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString('fr-FR', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric'
    });
  }
</script>

<AppDialog
  open={true}
  title="تفاصيل حركة المخزون"
  description={formatDate(movement.timestamp)}
  size="lg"
  on:close={close}
>
  <div class="grid grid-cols-2 gap-4 mb-6">
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">المنتج</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{movement.product_name || movement.product_id}</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">نوع الحركة</p>
      <p class="text-lg font-bold mt-1">
        {#if movement.movement_type === 'IN'}
          <AppBadge intent="success">دخول</AppBadge>
        {:else if movement.movement_type === 'OUT'}
          <AppBadge intent="danger">خروج</AppBadge>
        {:else if movement.movement_type === 'OPENING'}
          <AppBadge intent="info">افتتاحي</AppBadge>
        {:else}
          <AppBadge intent="neutral">{movement.movement_type}</AppBadge>
        {/if}
      </p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">الكمية</p>
      <p class="text-lg font-bold text-civil-blue">{movement.quantity.toFixed(2)}</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">الرصيد بعد</p>
      <p class="text-lg font-bold text-civil-blue">{movement.balance_after.toFixed(2)}</p>
    </div>
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">المستخدم</p>
      <p class="text-lg font-bold text-gray-900 dark:text-white">{movement.username}</p>
    </div>
    {#if movement.unit_id}
      <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
        <p class="text-sm text-gray-500 dark:text-gray-400">الوحدة</p>
        <p class="text-lg font-bold text-gray-900 dark:text-white">{movement.unit_id}</p>
      </div>
    {/if}
  </div>

  {#if movement.notes}
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg mb-4">
      <p class="text-sm text-gray-500 dark:text-gray-400">ملاحظات</p>
      <p class="text-base text-gray-900 dark:text-white">{movement.notes}</p>
    </div>
  {/if}
  
  {#if movement.reference_type}
    <div class="p-3 bg-gray-50 dark:bg-gray-900 rounded-lg">
      <p class="text-sm text-gray-500 dark:text-gray-400">المرجع</p>
      <p class="text-base text-gray-900 dark:text-white">{movement.reference_type}: {movement.reference_id || '-'}</p>
    </div>
  {/if}

  <svelte:fragment slot="actions">
    <AppButton variant="secondary" on:click={close}>إغلاق</AppButton>
  </svelte:fragment>
</AppDialog>
