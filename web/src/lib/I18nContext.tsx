import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  intlLocale,
  loadLocale,
  setLocalePreference,
  translate,
  translateLabel,
  type Locale,
  type TranslationKey,
} from './i18n';

interface I18nValue {
  locale: Locale;
  intl: string;
  setLocale: (next: Locale) => void;
  t: (key: TranslationKey, vars?: Record<string, string | number>) => string;
  tl: (raw: string) => string;
}

const DEFAULT_VALUE: I18nValue = {
  locale: 'en',
  intl: 'en-US',
  setLocale: (next) => setLocalePreference(next),
  t: (key, vars) => translate('en', key, vars),
  tl: (raw) => raw,
};

const I18nContext = createContext<I18nValue>(DEFAULT_VALUE);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(loadLocale);

  useEffect(() => {
    const onExternal = (event: Event) => {
      const detail = (event as CustomEvent<Locale>).detail;
      if (detail === 'en' || detail === 'zh-CN') setLocaleState(detail);
    };
    window.addEventListener('cowiki:locale', onExternal);
    return () => window.removeEventListener('cowiki:locale', onExternal);
  }, []);

  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next);
    setLocalePreference(next);
  }, []);

  const value = useMemo<I18nValue>(() => ({
    locale,
    intl: intlLocale(locale),
    setLocale,
    t: (key, vars) => translate(locale, key, vars),
    tl: (raw) => translateLabel(locale, raw),
  }), [locale, setLocale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  return useContext(I18nContext);
}
