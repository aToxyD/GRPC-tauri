/**
 * Single source of truth for the WebView2 CDP port used by the runtime E2E
 * suite.
 *
 * The port is FIXED, not dynamically allocated, because it is compiled into
 * the application binary: the window's `additionalBrowserArgs` is baked in by
 * `tauri-build` at compile time (see `scripts/e2e/tauri_cdp_overlay.ts`).
 * A build-time port cannot be negotiated at run time, so the build overlay and
 * the test driver must agree on a single value. They share this module to make
 * that agreement structural rather than conventional.
 *
 * GRPC_E2E_CDP_PORT overrides the default for anyone who needs to run two
 * isolated E2E instances side by side.
 */
export const DEFAULT_CDP_PORT = 9222;

export const CDP_PORT_ENV_VAR = 'GRPC_E2E_CDP_PORT';

/**
 * Resolves the CDP port from the environment, falling back to the default.
 * Invalid values fail fast rather than silently falling back, so a typo cannot
 * make the harness wait on a port the binary was never built to open.
 */
export function resolveCdpPort(env: NodeJS.ProcessEnv = process.env): number {
  const raw = env[CDP_PORT_ENV_VAR];
  if (!raw) return DEFAULT_CDP_PORT;
  const port = Number.parseInt(raw, 10);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error(`${CDP_PORT_ENV_VAR} must be a port number in 1024..65535, got: ${raw}`);
  }
  return port;
}
