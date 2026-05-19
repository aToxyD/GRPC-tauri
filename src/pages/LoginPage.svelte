<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { login, getSettings, isConfigured, importUnitNodePackage } from '../lib/tauri';
  import type { LoginRequest, LoginResponse } from '../lib/types';
  import { push } from 'svelte-spa-router';
  import { showSuccess } from '../lib/notifications';
  import { setCurrentUser } from '../lib/session';
  import AppButton from '../lib/components/ui/AppButton.svelte';
  import AppCard from '../lib/components/ui/AppCard.svelte';
  import AppInput from '../lib/components/ui/AppInput.svelte';
  import AppAlert from '../lib/components/ui/AppAlert.svelte';

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
      if (await window.isMaximized()) await window.unmaximize();
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
        filters: [{ name: 'حزمة التكوين', extensions: ['unit'] }],
      });
      if (selected) {
        await importUnitNodePackage(selected as string);
        isAppConfigured = true;
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
            if (settings?.node_type === 'WILAYA') push('/wilaya');
            else if (settings?.node_type === 'UNIT') push('/unit');
            else push('/configure');
          } catch {
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
          if (timeMatch) lockoutTimeRemaining = parseInt(timeMatch[1]);
        } else {
          loginAttempts++;
          remainingAttempts = Math.max(0, 5 - loginAttempts);
          if (loginAttempts >= 5) { isRateLimited = true; lockoutTimeRemaining = 5; }
        }
      }
    } catch (e) {
      error = 'خطأ في تسجيل الدخول: ' + String(e);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e: CustomEvent<KeyboardEvent> | KeyboardEvent) {
    const key = e instanceof KeyboardEvent ? e.key : (e as CustomEvent<KeyboardEvent>).detail?.key ?? '';
    if (key === 'Enter') handleLogin();
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900" dir="rtl">
  <div class="w-full max-w-md">
  <AppCard elevated padding="lg">

    <!-- الشعار والعنوان -->
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
      <div class="mb-4">
        <AppAlert intent="danger">{error}</AppAlert>
      </div>
    {/if}

    <!-- مؤشر محاولات الدخول -->
    {#if loginAttempts > 0}
      <div class="mb-4">
        <AppAlert intent={isRateLimited ? 'danger' : 'warning'}>
          <div class="flex items-center justify-between">
            <span>محاولات تسجيل الدخول: {loginAttempts}/5</span>
            {#if remainingAttempts !== null}
              <span class="font-semibold">
                {remainingAttempts > 0 ? `${remainingAttempts} متبقية` : 'محظور'}
              </span>
            {/if}
          </div>
          <div class="mt-2 w-full bg-current/20 rounded-full h-1.5" role="progressbar" aria-valuenow={loginAttempts} aria-valuemin={0} aria-valuemax={5}>
            <div
              class="h-1.5 rounded-full transition-all duration-300 {loginAttempts >= 4 ? 'bg-red-500' : loginAttempts >= 2 ? 'bg-yellow-500' : 'bg-green-500'}"
              style="width: {(loginAttempts / 5) * 100}%"
            ></div>
          </div>
        </AppAlert>
      </div>
    {/if}

    <!-- نموذج الدخول -->
    <form class="space-y-4" on:submit|preventDefault={handleLogin} novalidate>
      <AppInput
        id="username"
        label="اسم المستخدم"
        type="text"
        bind:value={username}
        placeholder="أدخل اسم المستخدم"
        autocomplete="username"
        required
        disabled={loading || isRateLimited}
        on:keydown={handleKeydown}
      />

      <AppInput
        id="password"
        label="كلمة المرور"
        type="password"
        bind:value={password}
        placeholder="أدخل كلمة المرور"
        autocomplete="current-password"
        required
        disabled={loading || isRateLimited}
        on:keydown={handleKeydown}
      />

      <AppButton
        type="submit"
        variant="primary"
        size="lg"
        fullWidth
        {loading}
        disabled={isRateLimited}
      >
        تسجيل الدخول
      </AppButton>
    </form>

    <!-- استيراد حزمة التكوين -->
    {#if !isAppConfigured}
      <div class="mt-6 pt-6 border-t border-gray-200 dark:border-gray-700">
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-3 text-center">لم يتم تكوين العقدة بعد</p>
        <AppButton
          variant="secondary"
          size="lg"
          fullWidth
          loading={importLoading}
          on:click={handleImportPackage}
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
          </svg>
          استيراد حزمة التكوين (.unit)
        </AppButton>
      </div>
    {:else}
      <p class="mt-6 text-center text-xs text-gray-400 dark:text-gray-500">
        بيانات الدخول الافتراضية: admin / admin
      </p>
    {/if}
  </AppCard>
  </div>
</div>
