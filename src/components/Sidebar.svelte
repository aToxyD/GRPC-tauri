<script lang="ts">
  import { onMount } from "svelte";
  import { link, push } from "svelte-spa-router";
  // [arch:allow-component-ipc-ghost] FE-138: Sidebar fetches settings directly as governance debt (21 pages need threading)
  import { getSettings } from "../lib/contracts";
  import type { User, Settings } from "../lib/types";
  import { currentUser as userStore, logout } from "../lib/session";
  import { get } from "svelte/store";
  import { theme, toggleTheme } from "../lib/theme";
  import { Sun, Moon } from "lucide-svelte";

  export let nodeType: "WILAYA" | "UNIT" | null = null;
  export let displayType: "WILAYA" | "UNIT" | null = null;

  // @category SessionState
  let user: User | null = null;
  // @category SessionState
  $: user = $userStore;

  // Type guard to check if user is Admin
  // @category UiState
  $: isAdmin = user?.role === "Admin";
  // @category ProjectionState
  let settings: Settings | null = null;
  // @category UiState
  let loading = true;

  onMount(async () => {
    if (!user) {
      push("/login");
      return;
    }

    try {
      settings = await getSettings();

      // Redirect if node type doesn't match settings
      // ONLY perform this check if nodeType was explicitly provided (not null)
      if (settings && nodeType) {
        if (nodeType === "WILAYA" && settings.node_type !== "WILAYA") {
          push("/unit");
          return;
        }
        if (nodeType === "UNIT" && settings.node_type !== "UNIT") {
          push("/wilaya");
          return;
        }
      }
    } catch (e) {
      push("/login");
    } finally {
      loading = false;
    }
  });

  // ── Icons (shared) ──────────────────────────────────────────────────────────
  const ICONS = {
    dashboard:
      "M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z",
    stats:
      "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6",
    products: "M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4",
    units:
      "M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4",
    reports:
      "M9 17v-2m3 2v-4m3 4v-6m2 10H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z",
    sync: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15",
    backup:
      "M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12",
    stock: "M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4",
    orders:
      "M3 3h2l.4 2M7 13h10l4-8H5.4M7 13L5.4 5M7 13l-2.293 2.293c-.63.63-.184 1.707.707 1.707H17m0 0a2 2 0 100 4 2 2 0 000-4zm-8 2a2 2 0 11-4 0 2 2 0 014 0z",
    consumption:
      "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01",
    auditLog:
      "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01",
    auditChain:
      "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z",
    health:
      "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z",
    topology: "M13 10V3L4 14h7v7l9-11h-7z",
    conflicts:
      "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
    fiscal:
      "M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z",
    settings:
      "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065zM15 12a3 3 0 11-6 0 3 3 0 016 0z",
    diagnostics:
      "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01",
  };

  // ── Link definitions ────────────────────────────────────────────────────────

  // Section type: either a plain link or a section header
  type NavItem =
    | { kind: "link"; path: string; label: string; icon: string }
    | { kind: "header"; label: string };

  function makeLink(path: string, label: string, icon: string): NavItem {
    return { kind: "link", path, label, icon };
  }
  function makeHeader(label: string): NavItem {
    return { kind: "header", label };
  }

  // ── Admin-only observability section (shared by both node types) ────────────
  // All audit/observability pages require Admin role.
  // @category UiState — navigation links
  let adminObservabilityLinks: NavItem[] = [];
  // @category UiState — navigation links
  $: {
    const links: NavItem[] = [];

    if (isAdmin) {
      links.push(makeHeader("المراقبة والأمان"));
      links.push(makeLink("/audit-log", "سجل التدقيق", ICONS.auditLog));
      links.push(
        makeLink("/admin/audit-integrity", "سلامة التدقيق", ICONS.auditChain),
      );
      links.push(makeLink("/admin/system-health", "صحة النظام", ICONS.health));
      links.push(
        makeLink("/admin/diagnostics", "التشخيصات المتقدمة", ICONS.diagnostics),
      );

      // Only add topology and conflicts for Wilaya node
      if (effectiveNodeType === "WILAYA") {
        links.push(
          makeLink(
            "/admin/sync-topology",
            "طوبولوجيا المزامنة",
            ICONS.topology,
          ),
        );
        links.push(
          makeLink("/admin/conflicts", "مركز التعارضات", ICONS.conflicts),
        );
      }
    }

    adminObservabilityLinks = links;
  }

  // ── Settings link (SEC-014) ─────────────────────────────────────────────────
  // UX visibility only — authorization stays backend-authoritative.
  // Visible: WILAYA Admin, UNIT Admin, UNIT User. Hidden: WILAYA User.
  // @category UiState — navigation links
  let settingsLinks: NavItem[] = [];
  // @category UiState
  $: {
    settingsLinks =
      effectiveNodeType === "UNIT" || isAdmin
        ? [makeLink("/settings", "الإعدادات", ICONS.settings)]
        : [];
  }

  // ── WILAYA nav ──────────────────────────────────────────────────────────────
  // @category UiState — navigation links
  let wilayaLinks: NavItem[] = [];
  // @category UiState
  $: wilayaLinks = [
    makeHeader("الإدارة"),
    makeLink("/wilaya", "نظرة عامة", ICONS.dashboard),
    makeLink("/wilaya/statistics", "الإحصائيات", ICONS.stats),
    makeLink("/admin/fiscal", "تسيير السنة المالية", ICONS.fiscal),
    makeLink("/wilaya/products", "المنتجات", ICONS.products),
    makeLink("/wilaya/units", "الوحدات", ICONS.units),
    makeLink("/wilaya/suppliers", "الموردون", ICONS.orders),
    makeLink("/wilaya/contracts", "العقود", ICONS.orders),
    makeLink("/wilaya/unit-inventory", "مخزون الوحدات", ICONS.stock),
    makeLink("/wilaya/reports", "التقارير", ICONS.reports),
    makeLink("/wilaya/sync", "المزامنة", ICONS.sync),
    makeLink("/backup", "النسخ الاحتياطية", ICONS.backup),
    ...settingsLinks,
    ...adminObservabilityLinks,
  ];

  // ── UNIT nav ────────────────────────────────────────────────────────────────
  // @category UiState — navigation links
  let unitLinks: NavItem[] = [];
  // @category UiState
  $: unitLinks = [
    makeHeader("العمليات"),
    makeLink("/unit", "نظرة عامة", ICONS.dashboard),
    makeLink("/unit/statistics", "الإحصائيات", ICONS.stats),
    makeLink("/admin/fiscal", "تسيير السنة المالية", ICONS.fiscal),
    makeLink("/unit/stock", "المخزون", ICONS.stock),
    makeLink("/unit/orders", "الطلبيات", ICONS.orders),
    makeLink("/unit/consumption", "الاستهلاك", ICONS.consumption),
    makeLink("/unit/reports", "التقارير", ICONS.reports),
    makeLink("/backup", "النسخ الاحتياطية", ICONS.backup),
    ...settingsLinks,
    ...adminObservabilityLinks,
  ];

  // Determine effective node type for links
  // @category UiState
  $: effectiveNodeType =
    nodeType || displayType || settings?.node_type || "WILAYA";
  // @category UiState
  $: navLinks = effectiveNodeType === "WILAYA" ? wilayaLinks : unitLinks;

  // @category UiState — active route
  let currentPath = window.location.hash.replace("#", "") || "/";

  function isActive(path: string): boolean {
    return currentPath === path || currentPath.startsWith(path + "/");
  }

  async function handleLogout() {
    await logout();
  }
</script>

{#if !loading && user && settings}
  <aside
    class="w-64 bg-white dark:bg-gray-900 shadow-lg min-h-screen flex flex-col flex-shrink-0 transition-colors duration-200"
  >
    <!-- Logo & Theme Toggle -->
    <div class="p-6 border-b border-gray-100 dark:border-gray-800">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
        <div
          class="w-10 h-10 bg-civil-blue rounded-lg flex items-center justify-center"
        >
          <svg
            class="w-6 h-6 text-white"
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
        <div>
          <h1 class="font-bold text-gray-800 dark:text-gray-100">GRPC</h1>
          <p class="text-xs text-gray-500 dark:text-gray-400">
            {nodeType === "WILAYA"
              ? settings.wilaya_name || "الولاية"
              : settings.unit_name || "الوحدة"}
          </p>
        </div>
        </div>
        <button
          on:click={toggleTheme}
          class="p-2 rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:bg-gray-700 dark:text-gray-400 dark:hover:bg-gray-800 transition-colors focus:outline-none focus:ring-2 focus:ring-civil-blue"
          title={$theme === "dark" ? "التبديل للمظهر الفاتح" : "التبديل للمظهر الداكن"}
          aria-label="Toggle Theme"
        >
          {#if $theme === "dark"}
            <Moon size={20} />
          {:else}
            <Sun size={20} />
          {/if}
        </button>
      </div>
    </div>

    <!-- Navigation -->
    <nav class="flex-1 p-4 overflow-y-auto">
      <ul class="space-y-0.5">
        {#each navLinks as item}
          {#if item.kind === "header"}
            <li class="pt-4 pb-1 first:pt-0">
              <span
                class="px-3 text-[10px] font-bold uppercase tracking-widest text-gray-400 dark:text-gray-500 dark:text-gray-400 select-none"
              >
                {item.label}
              </span>
            </li>
          {:else}
            <li>
              <a
                href={item.path}
                use:link
                class="flex items-center gap-3 px-3 py-2.5 rounded-lg transition-all duration-150 {isActive(
                  item.path,
                )
                  ? 'bg-civil-blue text-white shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:bg-gray-700 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-white'}"
              >
                <svg
                  class="w-4.5 h-4.5 flex-shrink-0"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d={item.icon}
                  />
                </svg>
                <span class="text-sm font-medium">{item.label}</span>
              </a>
            </li>
          {/if}
        {/each}
      </ul>
    </nav>

    <!-- User & Logout -->
    <div class="p-4 border-t border-gray-100 dark:border-gray-800">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div
            class="w-8 h-8 bg-gray-200 dark:bg-gray-700 rounded-full flex items-center justify-center"
          >
            <svg
              class="w-4 h-4 text-gray-500 dark:text-gray-400"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"
              />
            </svg>
          </div>
          <span class="text-sm font-medium text-gray-700 dark:text-gray-200">{user.username}</span>
        </div>
        <button
          on:click={handleLogout}
          class="p-2 text-gray-400 hover:text-red-500 dark:hover:text-red-400 transition-colors"
          title="تسجيل الخروج"
        >
          <svg
            class="w-5 h-5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"
            />
          </svg>
        </button>
      </div>
    </div>
  </aside>
{/if}
