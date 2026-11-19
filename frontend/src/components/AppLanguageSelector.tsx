'use client';

import { useTranslation } from 'react-i18next';

import { SUPPORTED_LANGUAGES, type AppLanguageCode } from '@/i18n';
import { useAppLanguage } from '@/contexts/LanguageContext';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

/**
 * Interface language picker. Writes the choice through the language provider,
 * which persists it and retranslates the app immediately.
 */
export function AppLanguageSelector() {
  const { t } = useTranslation();
  const { language, setLanguage } = useAppLanguage();

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between gap-6">
        <div>
          <h3 className="text-lg font-semibold text-gray-900 mb-2">
            {t('appLanguage.title')}
          </h3>
          <p className="text-sm text-gray-600">{t('appLanguage.description')}</p>
        </div>
        <Select
          value={language}
          onValueChange={(value) => setLanguage(value as AppLanguageCode)}
        >
          <SelectTrigger className="w-56">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SUPPORTED_LANGUAGES.map((option) => (
              <SelectItem key={option.code} value={option.code}>
                <span lang={option.code}>{option.label}</span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  );
}
