/** Tauri bridge for Easy Mode — same pattern as PortfolioPage's local helper:
 *  the dynamic import keeps the browser preview working without the shell. */

export const easyHasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function easyInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
