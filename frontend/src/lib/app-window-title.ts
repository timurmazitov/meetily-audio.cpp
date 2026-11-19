/**
 * Keeps the window/document title in step with the interface language
 * (spec decision 7). `src/app/metadata.ts` stays English as the build-time
 * fallback; the live title is set from the client on every language switch.
 */

/**
 * Sets the document title and, under Tauri, the native window title.
 *
 * The Tauri import is dynamic and guarded so the statically exported browser
 * build never touches Tauri APIs (same pattern as `src/lib/app-language.ts`).
 * A failure to set the native title never throws: the document title is
 * already correct and a broken window title must not break the app.
 */
export async function applyWindowTitle(title: string): Promise<void> {
  if (typeof document !== 'undefined') {
    document.title = title;
  }

  // Under Tauri the webview's document title does not reach the window
  // chrome; the native title has to be set through the window API. Guard on
  // the injected Tauri internals (same detection as `usePlatform`) so the
  // browser build skips the import entirely.
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setTitle(title);
    } catch (error) {
      // A title that cannot follow the language is cosmetic; it must never
      // break the language switch itself.
      console.error('Failed to set window title:', error);
    }
  }
}
