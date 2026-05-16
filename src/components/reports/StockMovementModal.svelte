<script lang="ts">
  import type { StockMovement } from '../../lib/types';
  import { createEventDispatcher } from 'svelte';

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

<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
  <div class="bg-white rounded-lg shadow-xl w-full max-w-2xl mx-4">
    <div class="p-6 border-b border-gray-100 flex items-center justify-between">
      <div>
        <h2 class="text-xl font-semibold text-gray-800">تفاصيل حركة المخزون</h2>
        <p class="text-sm text-gray-500">{formatDate(movement.timestamp)}</p>
      </div>
      <button on:click={close} class="text-gray-400 hover:text-gray-600" aria-label="إغلاق">
        <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
        </svg>
      </button>
    </div>
    <div class="p-6">
      <div class="grid grid-cols-2 gap-4 mb-6">
        <div class="p-3 bg-gray-50 rounded-lg">
          <p class="text-sm text-gray-500">المنتج</p>
          <p class="text-lg font-bold">{movement.product_name || movement.product_id}</p>
        </div>
        <div class="p-3 bg-gray-50 rounded-lg">
          <p class="text-sm text-gray-500">نوع الحركة</p>
          <p class="text-lg font-bold">
            {#if movement.movement_type === 'IN'}<span class="text-green-600">دخول</span>
            {:else if movement.movement_type === 'OUT'}<span class="text-red-600">خروج</span>
            {:else if movement.movement_type === 'OPENING'}<span class="text-blue-600">افتتاحي</span>
            {/if}
          </p>
        </div>
        <div class="p-3 bg-gray-50 rounded-lg">
          <p class="text-sm text-gray-500">الكمية</p>
          <p class="text-lg font-bold text-civil-blue">{movement.quantity.toFixed(2)}</p>
        </div>
        <div class="p-3 bg-gray-50 rounded-lg">
          <p class="text-sm text-gray-500">الرصيد بعد</p>
          <p class="text-lg font-bold text-civil-blue">{movement.balance_after.toFixed(2)}</p>
        </div>
        <div class="p-3 bg-gray-50 rounded-lg">
          <p class="text-sm text-gray-500">المستخدم</p>
          <p class="text-lg font-bold">{movement.username}</p>
        </div>
        {#if movement.unit_id}
          <div class="p-3 bg-gray-50 rounded-lg">
            <p class="text-sm text-gray-500">الوحدة</p>
            <p class="text-lg font-bold">{movement.unit_id}</p>
          </div>
        {/if}
      </div>
      {#if movement.notes}
        <div class="p-3 bg-gray-50 rounded-lg mb-4">
          <p class="text-sm text-gray-500">ملاحظات</p>
          <p class="text-base">{movement.notes}</p>
        </div>
      {/if}
      {#if movement.reference_type}
        <div class="p-3 bg-gray-50 rounded-lg">
          <p class="text-sm text-gray-500">المرجع</p>
          <p class="text-base">{movement.reference_type}: {movement.reference_id || '-'}</p>
        </div>
      {/if}
    </div>
  </div>
</div>
