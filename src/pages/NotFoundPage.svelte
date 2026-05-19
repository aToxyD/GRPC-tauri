<script lang="ts">
  import { push } from 'svelte-spa-router';
  import AppButton from '../lib/components/ui/AppButton.svelte';
</script>

<div class="min-h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-900" dir="rtl">
  <div class="text-center p-8 bg-white dark:bg-gray-800 rounded-2xl shadow-xl max-w-md w-full border border-gray-100 dark:border-gray-700">
    <div class="w-24 h-24 bg-gray-100 dark:bg-gray-700 rounded-full flex items-center justify-center mx-auto mb-6">
      <svg class="w-12 h-12 text-gray-400 dark:text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
    </div>
    <h1 class="text-6xl font-bold text-gray-800 dark:text-gray-100 mb-4">404</h1>
    <h2 class="text-2xl font-semibold text-gray-700 dark:text-gray-300 mb-4">الصفحة غير موجودة</h2>
    <p class="text-gray-500 dark:text-gray-400 mb-8">عذراً، الصفحة التي تحاول الوصول إليها غير موجودة أو تم تغيير مسارها.</p>
    
    <div class="flex justify-center">
      <AppButton
        variant="primary"
        size="lg"
        on:click={() => push('/')}
      >
        <svg class="w-5 h-5 ml-2 inline-block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18" />
        </svg>
        العودة للصفحة الرئيسية
      </AppButton>
    </div>
  </div>
</div>
