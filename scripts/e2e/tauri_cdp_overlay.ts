/**
 * Emits a TAURI_CONFIG overlay that enables the WebView2 remote-debugging (CDP)
 * endpoint for the runtime E2E build on Windows.
 *
 * WHY THIS EXISTS
 * ---------------
 * The E2E harness cannot reach the Tauri webview through the
 * `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment variable. wry builds the
 * WebView2 environment options and *always* calls
 * `set_additional_browser_arguments(...)` with a non-empty value
 * (wry-0.55.1 `src/webview2/mod.rs`, the `pl_attrs.additional_browser_args`
 * branch). Per the WebView2 contract, an app-supplied
 * `AdditionalBrowserArguments` value OVERRIDES the
 * `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment variable rather than
 * extending it, so the flag set by the harness is discarded silently and the
 * CDP port never opens. The application itself is unaffected — it boots,
 * migrates and reports READY — only the debug port is missing.
 *
 * The only supported way to reach WebView2's debug port is therefore the
 * `additionalBrowserArgs` window attribute, which wry does honour. That value
 * is compiled into the binary, so it is injected here at build time instead of
 * at run time.
 *
 * DESIGN CONSTRAINTS
 * ------------------
 * - `tauri.conf.json` is never modified. This script reads it and prints an
 *   overlay, so `tauri.conf.json` stays the single source of truth for the
 *   shipped window definition.
 * - `tauri-build` applies TAURI_CONFIG through `json_patch::merge`
 *   (RFC 7386 JSON Merge Patch), which REPLACES arrays wholesale. The overlay
 *   therefore carries the complete, unmodified main window plus the injected
 *   `additionalBrowserArgs` — the window definition is never partially
 *   specified.
 * - Release behaviour is untouched. Release builds do not set TAURI_CONFIG, and
 *   the overlay is emitted only on Windows (the only platform with WebView2).
 *   No production source file is changed.
 *
 * USAGE
 * -----
 *   TAURI_CONFIG="$(bun scripts/e2e/tauri_cdp_overlay.ts)" cargo build
 *
 * The CDP port is fixed at build time because the window argument is compiled
 * into the binary. It defaults to 9222 and can be overridden with
 * GRPC_E2E_CDP_PORT; `src/tests/e2e/helpers/cdpPort.ts` owns that resolution
 * and is shared with the test driver, so the harness and the binary cannot
 * disagree.
 */
import { readFileSync } from "fs";
import { join } from "path";
import { resolveCdpPort } from "../../src/tests/e2e/helpers/cdpPort";

const CONFIG_PATH = join(import.meta.dir, "..", "..", "src-tauri", "tauri.conf.json");

/**
 * wry replaces (never extends) this list when the app supplies its own
 * `additionalBrowserArgs`. Reproducing wry's default keeps the E2E build's
 * WebView2 behaviour aligned with a normal build instead of silently dropping
 * the mini-menu / PDF / SmartScreen suppressions. Kept in sync with
 * wry-0.55.1 `src/webview2/mod.rs`.
 */
const WRY_DEFAULT_BROWSER_ARGS = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/** WebView2 is Windows-only; emit a no-op overlay everywhere else. */
function isWebView2Platform(): boolean {
  return process.platform === "win32";
}

type TauriConfig = {
  app?: { windows?: Record<string, unknown>[] };
};

/**
 * Locates the window the E2E suite attaches to. `main.rs` resolves the primary
 * window as "main", and a Tauri window without an explicit label defaults to
 * that same label, so an unlabelled first window is the correct target.
 */
function findMainWindow(windows: Record<string, unknown>[]): Record<string, unknown> {
  const main = windows.find((w) => w.label === "main");
  if (main) return main;
  if (windows.length === 1) return windows[0];
  throw new Error(
    "Cannot resolve the main window for the CDP overlay: expected one window, " +
      `or a window labelled "main", but tauri.conf.json declares ${windows.length} windows. ` +
      "Set an explicit \"label\": \"main\" so the E2E target is unambiguous.",
  );
}

function main(): void {
  if (!isWebView2Platform()) {
    process.stdout.write("{}");
    return;
  }

  const cdpPort = resolveCdpPort();
  const config = JSON.parse(readFileSync(CONFIG_PATH, "utf-8")) as TauriConfig;
  const windows = config.app?.windows;

  if (!Array.isArray(windows) || windows.length === 0) {
    throw new Error(`No app.windows found in ${CONFIG_PATH}; the CDP overlay cannot be built.`);
  }

  const mainWindow = findMainWindow(windows);
  const window = {
    ...mainWindow,
    additionalBrowserArgs: `${WRY_DEFAULT_BROWSER_ARGS} --remote-debugging-port=${cdpPort}`,
  };

  // RFC 7386 replaces the whole array, so emit every window with only the
  // target one modified.
  const overlay = {
    app: {
      windows: windows.map((w) => (w === mainWindow ? window : w)),
    },
  };

  process.stdout.write(JSON.stringify(overlay));
}

main();
