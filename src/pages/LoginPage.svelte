<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { login, getSettings, isConfigured, importUnitNodePackage } from '../lib/tauri';
  import type { LoginRequest, LoginResponse, User } from '../lib/types';
  import { push } from 'svelte-spa-router';
  import { showSuccess } from '../lib/notifications';
  import { setCurrentUser } from '../lib/session';

  let username = '';
  let password = '';
  let error = '';
  let loading = false;
  let isAppConfigured = true;
  let importLoading = false;

  let loginAttempts = 0;
  let remainingAttempts: number | null = null;
  let lockoutTimeRemaining: number | null = null;
  let isRateLimited = false;

  onMount(async () => {
    try {
      isAppConfigured = await isConfigured();
    } catch (e) {
      isAppConfigured = false;
    }

    try {
      const window = getCurrentWindow();
      await window.setResizable(true);
      await window.setMaximizable(true);
      if (await window.isMaximized()) {
        await window.unmaximize();
      }
      await window.setSize(new LogicalSize(450, 650));
      await window.setResizable(false);
      await window.setMaximizable(false);
      await window.center();
    } catch (err) {
      console.error('Failed to configure login window size:', err);
    }
  });

  async function handleImportPackage() {
    try {
      importLoading = true;
      error = '';

      const selected = await open({
        multiple: false,
        filters: [{
          name: 'حزمة التكوين',
          extensions: ['unit']
        }]
      });

      if (selected) {
        await importUnitNodePackage(selected as string);
        isAppConfigured = true;
        error = '';
        showSuccess('تم استيراد حزمة التكوين بنجاح! يمكنك الآن تسجيل الدخول.');
      }
    } catch (e) {
      error = 'خطأ في استيراد الحزمة: ' + String(e);
    } finally {
      importLoading = false;
    }
  }

  async function handleLogin() {
    if (!username || !password) {
      error = 'الرجاء إدخال اسم المستخدم وكلمة المرور';
      return;
    }

    if (isRateLimited) {
      error = `تم حظر تسجيل الدخول مؤقتاً. انتظر ${lockoutTimeRemaining || 5} دقائق`;
      return;
    }

    loading = true;
    error = '';

    try {
      const request: LoginRequest = { username, password };
      const response: LoginResponse = await login(request);

      if (response.success && response.user) {
        loginAttempts = 0;
        remainingAttempts = null;
        isRateLimited = false;
        setCurrentUser(response.user);

        if (response.requires_configuration) {
          push('/configure?nodeType=WILAYA');
        } else {
          try {
            const window = getCurrentWindow();
            await window.setResizable(true);
            await window.setMaximizable(true);
            await window.maximize();
          } catch (err) {
            console.error('Failed to maximize window:', err);
          }

          try {
            const settings = await getSettings();
            if (settings && settings.node_type === 'WILAYA') {
              push('/wilaya');
            } else if (settings && settings.node_type === 'UNIT') {
              push('/unit');
            } else {
              push('/configure');
            }
          } catch (settingsError) {
            push('/configure');
          }
        }
      } else {
        error = response.message || 'بيانات الدخول غير صالحة';

        if (response.message.includes('تجاوز الحد')) {
          isRateLimited = true;
          loginAttempts = 5;
          remainingAttempts = 0;
          const timeMatch = response.message.match(/(\d+) دقيقة/);
          if (timeMatch) {
            lockoutTimeRemaining = parseInt(timeMatch[1]);
          }
        } else {
          loginAttempts++;
          remainingAttempts = Math.max(0, 5 - loginAttempts);
          if (loginAttempts >= 5) {
            isRateLimited = true;
            lockoutTimeRemaining = 5;
          }
        }
      }
    } catch (e) {
      error = 'خطأ في تسجيل الدخول: ' + String(e);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      handleLogin();
    }
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900" dir="rtl">
  <div class="card w-full max-w-md p-8 shadow-xl dark:shadow-gray-950/50 dark:bg-gray-800 dark:border-gray-700">

    <!-- العنوان والشعار -->
    <div class="text-center mb-8">
      <div class="w-16 h-16 bg-civil-blue rounded-full flex items-center justify-center mx-auto mb-4 shadow-lg" role="img" aria-label="شعار نظام GRPC">
        <svg class="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"/>
        </svg>
      </div>
      <h1 class="text-2xl font-bold text-gray-900 dark:text-white">GRPC</h1>
      <p class="text-gray-500 dark:text-gray-400 mt-1 text-sm">نظام إدارة مطاعم الحماية المدنية</p>
    </div>

    <!-- رسالة الخطأ -->
    {#if error}
      <div
        role="alert"
        class="mb-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-red-700 dark:text-red-300 text-sm"
      >
        <span class="font-medium">⚠ </span>{error}
      </div>
    {/if}

    <!-- مؤشر محاولات الدخول -->
    {#if loginAttempts > 0}
      <div
        role="status"
        aria-live="polite"
        class="mb-4 p-3 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg text-yellow-800 dark:text-yellow-300 text-sm"
      >
        <div class="flex items-center justify-between">
          <span>محاولات تسجيل الدخول: {loginAttempts}/5</span>
          {#if remainingAttempts !== null}
            <span class="font-semibold">
              {remainingAttempts > 0 ? `${remainingAttempts} محاولات متبقية` : 'تم الحظر'}
            </span>
          {/if}
        </div>
        <div class="mt-2" role="progressbar" aria-valuenow={loginAttempts} aria-valuemin={0} aria-valuemax={5} aria-label="مؤشر المحاولات">
          <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-1.5">
            <div
              class="h-1.5 rounded-full transition-all duration-300 {loginAttempts >= 4 ? 'bg-red-500' : loginAttempts >= 2 ? 'bg-yellow-500' : 'bg-green-500'}"
              style="width: {(loginAttempts / 5) * 100}%"
            ></div>
          </div>
        </div>
      </div>
    {/if}

    <!-- حالة الحظر -->
    {#if isRateLimited}
      <div
        role="alert"
        class="mb-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-red-700 dark:text-red-300 text-sm"
      >
        <div class="flex items-center gap-2">
          <svg class="w-5 h-5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"/>
          </svg>
          <span>تم حظر تسجيل الدخول مؤقتاً. انتظر {lockoutTimeRemaining || 5} دقائق.</span>
        </div>
      </div>
    {/if}

    <!-- نموذج تسجيل الدخول -->
    <form class="space-y-4" on:submit|preventDefault={handleLogin} novalidate>
      <div>
        <label for="username" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
          اسم المستخدم
        </label>
        <input
          id="username"
          type="text"
          autocomplete="username"
          class="input-field dark:bg-gray-700 dark:border-gray-600 dark:text-white dark:placeholder-gray-500"
          placeholder="أدخل اسم المستخدم"
          bind:value={username}
          on:keydown={handleKeydown}
          disabled={loading || isRateLimited}
          aria-required="true"
        />
      </div>

      <div>
        <label for="password" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
          كلمة المرور
        </label>
        <input
          id="password"
          type="password"
          autocomplete="current-password"
          class="input-field dark:bg-gray-700 dark:border-gray-600 dark:text-white dark:placeholder-gray-500"
          placeholder="أدخل كلمة المرور"
          bind:value={password}
          on:keydown={handleKeydown}
          disabled={loading || isRateLimited}
          aria-required="true"
        />
      </div>

      <button
        type="submit"
        class="w-full btn-primary py-3 font-semibold text-base mt-2"
        disabled={loading || isRateLimited}
        aria-busy={loading}
      >
        {#if loading}
          <span class="flex items-center justify-center gap-2">
            <svg class="animate-spin h-5 w-5 text-white" fill="none" viewBox="0 0 24 24" aria-hidden="true">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            جاري تسجيل الدخول...
          </span>
        {:else}
          تسجيل الدخول
        {/if}
      </button>
    </form>

    <!-- استيراد حزمة التكوين -->
    {#if !isAppConfigured}
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700">
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-3 text-center">لم يتم تكوين العقدة بعد</p>
        <button
          class="w-full btn-secondary py-3 font-medium"
          on:click={handleImportPackage}
          disabled={importLoading}
          type="button"
          aria-busy={importLoading}
        >
          {#if importLoading}
            <span class="flex items-center justify-center gap-2">
              <svg class="animate-spin h-5 w-5" fill="none" viewBox="0 0 24 24" aria-hidden="true">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              جاري الاستيراد...
            </span>
          {:else}
            <span class="flex items-center justify-center gap-2">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
              </svg>
              استيراد حزمة التكوين (.unit)
            </span>
          {/if}
        </button>
      </div>
    {:else}
      <div class="mt-6 text-center text-xs text-gray-400 dark:text-gray-500">
        <p>بيانات الدخول الافتراضية: admin / admin</p>
      </div>
    {/if}
  </div>
</div>
