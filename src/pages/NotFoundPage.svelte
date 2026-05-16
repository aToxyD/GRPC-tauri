<script lang="ts">
  import { push } from 'svelte-spa-router';
</script>

<div class="min-h-screen flex items-center justify-center bg-gray-50">
  <div class="text-center">
    <div class="w-24 h-24 bg-gray-200 rounded-full flex items-center justify-center mx-auto mb-6">
      <svg class="w-12 h-12 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
    </div>
    <h1 class="text-6xl font-bold text-gray-800 mb-4">404</h1>
    <h2 class="text-2xl font-semibold text-gray-600 mb-4">الصفحة غير موجودة</h2>
    <p class="text-gray-500 mb-8">الصفحة التي تبحث عنها غير موجودة أو تم نقلها</p>
    <button
      on:click={() => push('/')}
      class="btn-primary px-8 py-3"
    >
      العودة للصفحة الرئيسية
    </button>
  </div>
</div>
