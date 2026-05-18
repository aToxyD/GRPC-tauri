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
        // Reset login attempts on successful login
        loginAttempts = 0;
        remainingAttempts = null;
        isRateLimited = false;
        // Store user in session
        setCurrentUser(response.user);
        
        if (response.requires_configuration) {
          // Skip node selection and go directly to wilaya configuration in compact frame
          push('/configure?nodeType=WILAYA');
        } else {
          // Maximize window for main interface
          try {
            const window = getCurrentWindow();
            await window.setResizable(true);
            await window.setMaximizable(true);
            await window.maximize();
          } catch (err) {
            console.error('Failed to maximize window:', err);
          }
          
          // Get settings to determine where to redirect
          try {
            const settings = await getSettings();
            if (settings && settings.node_type === 'WILAYA') {
              push('/wilaya');
            } else if (settings && settings.node_type === 'UNIT') {
              push('/unit');
            } else {
              // Fallback to configuration if settings are invalid
              push('/configure');
            }
          } catch (settingsError) {
            push('/configure');
          }
        }
      } else {
        error = response.message || 'بيانات الدخول غير صالحة';
        
        // Handle rate limiting in error message
        if (response.message.includes('تجاوز الحد')) {
          isRateLimited = true;
          loginAttempts = 5; // Max attempts reached
          remainingAttempts = 0;
          
          // Extract lockout time from message if available
          const timeMatch = response.message.match(/(\d+) دقيقة/);
          if (timeMatch) {
            lockoutTimeRemaining = parseInt(timeMatch[1]);
          }
        } else {
          // Regular login failed - increment attempts
          loginAttempts++;
          remainingAttempts = Math.max(0, 5 - loginAttempts);
          
          if (loginAttempts >= 5) {
            isRateLimited = true;
            lockoutTimeRemaining = 5; // 5 minutes default
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

<div class="min-h-screen flex items-center justify-center bg-gradient-to-br from-civil-blue/5 to-civil-blue/10">
  <div class="card w-full max-w-md p-8">
    <div class="text-center mb-8">
      <div class="w-16 h-16 bg-civil-blue rounded-full flex items-center justify-center mx-auto mb-4">
        <svg class="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"/>
        </svg>
      </div>
      <h1 class="text-2xl font-bold text-gray-800">GRPC</h1>
      <p class="text-gray-600 mt-1">نظام إدارة مطاعم الحماية المدنية</p>
    </div>

    {#if error}
      <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
        {error}
      </div>
    {/if}

    <!-- Security Status Indicator -->
    {#if loginAttempts > 0}
      <div class="mb-4 p-3 bg-yellow-50 border border-yellow-200 rounded-lg text-yellow-800 text-sm">
        <div class="flex items-center justify-between">
          <span>محاولات تسجيل الدخول: {loginAttempts}/5</span>
          {#if remainingAttempts !== null}
            <span class="font-medium">
              {remainingAttempts > 0 ? `${remainingAttempts} محاولات متبقية` : 'تم حظر تسجيل الدخول'}
            </span>
          {/if}
        </div>
        <div class="mt-2">
          <div class="w-full bg-gray-200 rounded-full h-2">
            <div 
              class="h-2 rounded-full transition-all duration-300 {loginAttempts >= 4 ? 'bg-red-500' : loginAttempts >= 2 ? 'bg-yellow-500' : 'bg-green-500'}"
              style="width: {(loginAttempts / 5) * 100}%"
            ></div>
          </div>
        </div>
      </div>
    {/if}

    {#if isRateLimited}
      <div class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
        <div class="flex items-center">
          <svg class="w-5 h-5 ml-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"/>
          </svg>
          <span>تم حظر تسجيل الدخول مؤقتاً. انتظر {lockoutTimeRemaining || 5} دقائق.</span>
        </div>
      </div>
    {/if}

    <div class="space-y-4">
      <div>
        <label for="username" class="block text-sm font-medium text-gray-700 mb-1">اسم المستخدم</label>
        <input
          id="username"
          type="text"
          class="input-field"
          placeholder="أدخل اسم المستخدم"
          bind:value={username}
          on:keydown={handleKeydown}
        />
      </div>

      <div>
        <label for="password" class="block text-sm font-medium text-gray-700 mb-1">كلمة المرور</label>
        <input
          id="password"
          type="password"
          class="input-field"
          placeholder="أدخل كلمة المرور"
          bind:value={password}
          on:keydown={handleKeydown}
        />
      </div>



      <button
        class="w-full btn-primary py-3 font-medium disabled:opacity-50 disabled:cursor-not-allowed"
        on:click={handleLogin}
        disabled={loading}
      >
        {#if loading}
          <span class="flex items-center justify-center">
            <svg class="animate-spin -ml-1 mr-3 h-5 w-5 text-white" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            جاري تسجيل الدخول...
          </span>
        {:else}
          تسجيل الدخول
        {/if}
      </button>
    </div>

    {#if !isAppConfigured}
      <div class="mt-6 pt-6 border-t border-gray-200">
        <p class="text-sm text-gray-600 mb-3 text-center">لم يتم تكوين العقدة بعد</p>
        <button
          class="w-full btn-secondary py-3 font-medium disabled:opacity-50"
          on:click={handleImportPackage}
          disabled={importLoading}
        >
          {#if importLoading}
            <span class="flex items-center justify-center">
              <svg class="animate-spin -ml-1 mr-3 h-5 w-5 text-civil-blue" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              جاري الاستيراد...
            </span>
          {:else}
            <span class="flex items-center justify-center">
              <svg class="w-5 h-5 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
              </svg>
              استيراد حزمة التكوين (.unit)
            </span>
          {/if}
        </button>
      </div>
    {:else}
      <div class="mt-6 text-center text-sm text-gray-500">
        <p>افتراضي: admin / admin</p>
      </div>
    {/if}
  </div>
</div>
