<script lang="ts">
  import { onMount } from "svelte";
  import { configureAsWilaya, getSettings } from "../lib/tauri";
  import { push } from "svelte-spa-router";

  let wilayaCode = "";
  let wilayaName = "";
  let loading = false;
  let error = "";
  let success = "";

  onMount(async () => {
    // Check if already configured
    try {
      const settings = await getSettings();
      if (settings.configured) {
        if (settings.node_type === "WILAYA") {
          push("/wilaya");
        } else {
          push("/unit");
        }
      }
    } catch (e) {
      // Not configured yet, continue
    }
  });

  async function configureWilaya() {
    if (!wilayaCode || !wilayaName) {
      error = "الرجاء إدخال رمز واسم الولاية";
      return;
    }

    loading = true;
    error = "";
    success = "";

    try {
      await configureAsWilaya(wilayaCode, wilayaName);
      success = "تم تكوين الولاية بنجاح. جاري إعادة التوجيه...";
      setTimeout(() => push("/wilaya"), 1500);
    } catch (e) {
      error = "خطأ في التكوين: " + String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div
  class="min-h-screen flex items-center justify-center bg-gradient-to-br from-civil-blue/5 to-civil-blue/10"
>
  <div class="card w-full max-w-lg p-8">
    <div class="text-center mb-8">
      <div
        class="w-16 h-16 bg-civil-blue rounded-full flex items-center justify-center mx-auto mb-4"
      >
        <svg
          class="w-8 h-8 text-white"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4"
          />
        </svg>
      </div>
      <h1 class="text-2xl font-bold text-gray-800">تكوين الولاية</h1>
      <p class="text-gray-600 mt-1">قم بإعداد مديرية الولاية</p>
    </div>

    {#if error}
      <div
        class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm"
      >
        {error}
      </div>
    {/if}

    {#if success}
      <div
        class="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg text-green-700 text-sm"
      >
        {success}
      </div>
    {/if}

    <div class="space-y-4">
      <div>
        <label
          for="wilayaCode"
          class="block text-sm font-medium text-gray-700 mb-1"
          >رمز الولاية</label
        >
        <input
          id="wilayaCode"
          type="text"
          class="input-field"
          placeholder="مثال: 16 (الجزائر العاصمة)"
          bind:value={wilayaCode}
        />
      </div>

      <div>
        <label
          for="wilayaName"
          class="block text-sm font-medium text-gray-700 mb-1"
          >اسم الولاية</label
        >
        <input
          id="wilayaName"
          type="text"
          class="input-field"
          placeholder="مثال: الجزائر العاصمة"
          bind:value={wilayaName}
        />
      </div>

      <button
        class="w-full btn-primary py-3 font-medium disabled:opacity-50"
        on:click={configureWilaya}
        disabled={loading}
      >
        {#if loading}
          جاري التكوين...
        {:else}
          تكوين كولاية
        {/if}
      </button>
    </div>
  </div>
</div>
