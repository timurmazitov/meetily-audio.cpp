'use client';

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import i18n, {
  DEFAULT_LANGUAGE,
  normalizeAppLanguage,
  systemLocale,
  type AppLanguageCode,
} from '@/i18n';
import { applyWindowTitle } from '@/lib/app-window-title';
import { loadAppLanguage, saveAppLanguage } from '@/lib/app-language';

interface LanguageContextValue {
  language: AppLanguageCode;
  /** False until the stored preference (or the OS locale) has been resolved. */
  ready: boolean;
  setLanguage: (language: AppLanguageCode) => Promise<void>;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<AppLanguageCode>(DEFAULT_LANGUAGE);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const resolve = async () => {
      // A stored choice always wins. Without one, first run follows the OS
      // locale — `navigator.language` because tauri-plugin-os has no Rust half
      // installed in this app.
      let next: AppLanguageCode = normalizeAppLanguage(systemLocale());
      const stored = await loadAppLanguage();
      if (stored) next = stored;
      if (cancelled) return;

      if (i18n.language !== next) {
        await i18n.changeLanguage(next);
      }
      setLanguageState(next);
      setReady(true);
    };

    resolve();

    return () => {
      cancelled = true;
    };
  }, []);

  // Keep the document language in step with the interface language, for
  // hyphenation, spell-check and assistive technology.
  useEffect(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = language;
    }
  }, [language]);

  // Keep the window/document title in step with the interface language
  // (spec decision 7). `src/app/metadata.ts` stays English as the
  // build-time fallback; the live title is re-read from the catalog on
  // every switch so future titles localize with no further wiring.
  useEffect(() => {
    void applyWindowTitle(i18n.t('app.title'));
  }, [language]);

  const setLanguage = useCallback(async (next: AppLanguageCode) => {
    if (i18n.language !== next) {
      await i18n.changeLanguage(next);
    }
    setLanguageState(next);
    try {
      await saveAppLanguage(next);
    } catch (error) {
      // The switch itself already happened; a failed write only means the
      // choice will not survive a restart.
      console.error('Failed to save app language preference:', error);
    }
  }, []);

  const value = useMemo(
    () => ({ language, ready, setLanguage }),
    [language, ready, setLanguage],
  );

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

export function useAppLanguage(): LanguageContextValue {
  const context = useContext(LanguageContext);
  if (!context) {
    throw new Error('useAppLanguage must be used within a LanguageProvider');
  }
  return context;
}

/**
 * Renders its children once the interface language is known.
 *
 * The statically exported HTML is prerendered with the language provider's
 * default, so anything rendered before the stored preference is read can only
 * be in the wrong language. Holding the app back for the few milliseconds the
 * preference takes to load avoids both a flash of English and a hydration
 * mismatch.
 */
export function AppLanguageGate({ children }: { children: ReactNode }) {
  const { ready } = useAppLanguage();
  if (!ready) {
    return <div className="min-h-screen w-full" aria-busy="true" />;
  }
  return <>{children}</>;
}
