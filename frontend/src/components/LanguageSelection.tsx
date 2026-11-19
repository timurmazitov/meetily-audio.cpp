import React, { useState, useEffect } from 'react';
import { Globe } from 'lucide-react';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { useConfig } from '@/contexts/ConfigContext';
import { supportsTranslation } from '@/lib/transcription-provider';
import { useContentLanguageName } from '@/lib/content-languages';
export interface Language {
  code: string;
  name: string;
}

// ISO 639-1 language codes supported by Whisper
const LANGUAGES: Language[] = [
  { code: 'auto', name: 'Auto Detect (Original Language)' },
  { code: 'auto-translate', name: 'Auto Detect (Translate to English)' },
  { code: 'en', name: 'English' },
  { code: 'zh', name: 'Chinese' },
  { code: 'de', name: 'German' },
  { code: 'es', name: 'Spanish' },
  { code: 'ru', name: 'Russian' },
  { code: 'ko', name: 'Korean' },
  { code: 'fr', name: 'French' },
  { code: 'ja', name: 'Japanese' },
  { code: 'pt', name: 'Portuguese' },
  { code: 'tr', name: 'Turkish' },
  { code: 'pl', name: 'Polish' },
  { code: 'ca', name: 'Catalan' },
  { code: 'nl', name: 'Dutch' },
  { code: 'ar', name: 'Arabic' },
  { code: 'sv', name: 'Swedish' },
  { code: 'it', name: 'Italian' },
  { code: 'id', name: 'Indonesian' },
  { code: 'hi', name: 'Hindi' },
  { code: 'fi', name: 'Finnish' },
  { code: 'vi', name: 'Vietnamese' },
  { code: 'he', name: 'Hebrew' },
  { code: 'uk', name: 'Ukrainian' },
  { code: 'el', name: 'Greek' },
  { code: 'ms', name: 'Malay' },
  { code: 'cs', name: 'Czech' },
  { code: 'ro', name: 'Romanian' },
  { code: 'da', name: 'Danish' },
  { code: 'hu', name: 'Hungarian' },
  { code: 'ta', name: 'Tamil' },
  { code: 'no', name: 'Norwegian' },
  { code: 'th', name: 'Thai' },
  { code: 'ur', name: 'Urdu' },
  { code: 'hr', name: 'Croatian' },
  { code: 'bg', name: 'Bulgarian' },
  { code: 'lt', name: 'Lithuanian' },
  { code: 'la', name: 'Latin' },
  { code: 'mi', name: 'Maori' },
  { code: 'ml', name: 'Malayalam' },
  { code: 'cy', name: 'Welsh' },
  { code: 'sk', name: 'Slovak' },
  { code: 'te', name: 'Telugu' },
  { code: 'fa', name: 'Persian' },
  { code: 'lv', name: 'Latvian' },
  { code: 'bn', name: 'Bengali' },
  { code: 'sr', name: 'Serbian' },
  { code: 'az', name: 'Azerbaijani' },
  { code: 'sl', name: 'Slovenian' },
  { code: 'kn', name: 'Kannada' },
  { code: 'et', name: 'Estonian' },
  { code: 'mk', name: 'Macedonian' },
  { code: 'br', name: 'Breton' },
  { code: 'eu', name: 'Basque' },
  { code: 'is', name: 'Icelandic' },
  { code: 'hy', name: 'Armenian' },
  { code: 'ne', name: 'Nepali' },
  { code: 'mn', name: 'Mongolian' },
  { code: 'bs', name: 'Bosnian' },
  { code: 'kk', name: 'Kazakh' },
  { code: 'sq', name: 'Albanian' },
  { code: 'sw', name: 'Swahili' },
  { code: 'gl', name: 'Galician' },
  { code: 'mr', name: 'Marathi' },
  { code: 'pa', name: 'Punjabi' },
  { code: 'si', name: 'Sinhala' },
  { code: 'km', name: 'Khmer' },
  { code: 'sn', name: 'Shona' },
  { code: 'yo', name: 'Yoruba' },
  { code: 'so', name: 'Somali' },
  { code: 'af', name: 'Afrikaans' },
  { code: 'oc', name: 'Occitan' },
  { code: 'ka', name: 'Georgian' },
  { code: 'be', name: 'Belarusian' },
  { code: 'tg', name: 'Tajik' },
  { code: 'sd', name: 'Sindhi' },
  { code: 'gu', name: 'Gujarati' },
  { code: 'am', name: 'Amharic' },
  { code: 'yi', name: 'Yiddish' },
  { code: 'lo', name: 'Lao' },
  { code: 'uz', name: 'Uzbek' },
  { code: 'fo', name: 'Faroese' },
  { code: 'ht', name: 'Haitian Creole' },
  { code: 'ps', name: 'Pashto' },
  { code: 'tk', name: 'Turkmen' },
  { code: 'nn', name: 'Norwegian Nynorsk' },
  { code: 'mt', name: 'Maltese' },
  { code: 'sa', name: 'Sanskrit' },
  { code: 'lb', name: 'Luxembourgish' },
  { code: 'my', name: 'Myanmar' },
  { code: 'bo', name: 'Tibetan' },
  { code: 'tl', name: 'Tagalog' },
  { code: 'mg', name: 'Malagasy' },
  { code: 'as', name: 'Assamese' },
  { code: 'tt', name: 'Tatar' },
  { code: 'haw', name: 'Hawaiian' },
  { code: 'ln', name: 'Lingala' },
  { code: 'ha', name: 'Hausa' },
  { code: 'ba', name: 'Bashkir' },
  { code: 'jw', name: 'Javanese' },
  { code: 'su', name: 'Sundanese' },
];

interface LanguageSelectionProps {
  selectedLanguage: string;
  onLanguageChange: (language: string) => void;
  disabled?: boolean;
  provider?: 'localWhisper' | 'parakeet' | 'audioCpp' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
}

/**
 * Automatic translation is a Whisper-only capability: the local Whisper
 * decoder translates to English through its `translate` flag, while audioCpp
 * (GigaAM) and Parakeet only transcribe in the language they hear.
 */

/** The language option value that requests translation to English. */
const AUTO_TRANSLATE_CODE = 'auto-translate';

export function LanguageSelection({
  selectedLanguage,
  onLanguageChange,
  disabled = false,
  provider = 'localWhisper'
}: LanguageSelectionProps) {
  const { t } = useTranslation();
  const contentLanguageName = useContentLanguageName();
  const [saving, setSaving] = useState(false);
  const { setSelectedLanguage } = useConfig();

  /** One language's name in the interface language, e.g. «Английский». */
  const nameOf = (language: Language) => contentLanguageName(language.code, language.name);
  const translationWhisperOnly = t('languageSelection.translationWhisperOnly');

  // Parakeet only supports auto-detection (doesn't support manual language
  // selection), but it kept the auto-translate option before audioCpp landed,
  // so that behaviour is preserved verbatim.
  const isParakeet = provider === 'parakeet';
  // audioCpp transcribes in the model's trained language (Russian for GigaAM)
  // and is the only provider that loses the translation option.
  const isAudioCpp = provider === 'audioCpp';
  const translationAvailable = supportsTranslation(provider);
  const availableLanguages = isParakeet
    ? LANGUAGES.filter(lang => lang.code === 'auto' || lang.code === AUTO_TRANSLATE_CODE)
    : isAudioCpp
      ? LANGUAGES.filter(lang => lang.code !== AUTO_TRANSLATE_CODE)
      : LANGUAGES;

  const handleLanguageChange = async (languageCode: string) => {
    setSaving(true);
    try {
      // Save language preference to localStorage and sync to backend
      setSelectedLanguage(languageCode);
      onLanguageChange(languageCode);
      console.log('Language preference saved:', languageCode);

      // Show success toast
      const language = LANGUAGES.find(lang => lang.code === languageCode);
      const languageName = language ? nameOf(language) : languageCode;
      toast.success(t('languageSelection.saved'), {
        description: t('languageSelection.savedDescription', { language: languageName })
      });
    } catch (error) {
      console.error('Failed to save language preference:', error);
      toast.error(t('languageSelection.saveFailed'), {
        description: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setSaving(false);
    }
  };

  // Find the selected language name for display
  const selectedOption = LANGUAGES.find(lang => lang.code === selectedLanguage);
  const selectedLanguageName = selectedOption
    ? nameOf(selectedOption)
    : nameOf(LANGUAGES[0]);

  // audioCpp must not leave a stale translation preference active: the
  // transcription would arrive untranslated while the UI claims English
  // output. Parakeet's previous behaviour is untouched.
  const staleTranslation =
    isAudioCpp && selectedLanguage === AUTO_TRANSLATE_CODE && !translationAvailable;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Globe className="h-4 w-4 text-gray-600" />
          <h4 className="text-sm font-medium text-gray-900">{t('languageSelection.title')}</h4>
        </div>
      </div>

      <div className="space-y-2">
        <select
          value={staleTranslation ? 'auto' : selectedLanguage}
          onChange={(e) => handleLanguageChange(e.target.value)}
          disabled={disabled || saving}
          className="w-full px-3 py-2 text-sm bg-white border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 disabled:bg-gray-50 disabled:text-gray-500"
        >
          {availableLanguages.map((language) => (
            <option key={language.code} value={language.code}>
              {nameOf(language)}
              {language.code !== 'auto' && language.code !== AUTO_TRANSLATE_CODE && ` (${language.code})`}
            </option>
          ))}
        </select>

        {/* Parakeet language limitation warning */}
        {isParakeet && (
          <div className="p-2 bg-amber-50 border border-amber-200 rounded text-amber-800">
            <p className="font-medium">ℹ️ {t('languageSelection.parakeetTitle')}</p>
            <p className="mt-1 text-xs">{t('languageSelection.parakeetBody')}</p>
          </div>
        )}

        {/* audioCpp: translation is Whisper-only */}
        {isAudioCpp && (
          <div className="p-2 bg-amber-50 border border-amber-200 rounded text-amber-800" data-testid="audiocpp-translation-notice">
            <p className="font-medium">ℹ️ {t('languageSelection.audioCppTitle')}</p>
            <p className="mt-1 text-xs">
              {translationWhisperOnly}
            </p>
          </div>
        )}

        {/* Info text */}
        <div className="text-xs space-y-2 pt-2">
          <p className="text-gray-600">
            <strong>{t('languageSelection.current')}</strong> {selectedLanguageName}
          </p>
          {staleTranslation && (
            <div className="p-2 bg-amber-50 border border-amber-200 rounded text-amber-800">
              <p className="font-medium">⚠️ {t('languageSelection.translationUnavailable')}</p>
              <p className="mt-1">{translationWhisperOnly}</p>
            </div>
          )}
          {selectedLanguage === 'auto' && (
            <div className="p-2 bg-yellow-50 border border-yellow-200 rounded text-yellow-800">
              <p className="font-medium">⚠️ {t('languageSelection.autoWarningTitle')}</p>
              <p className="mt-1">{t('languageSelection.autoWarningBody')}</p>
            </div>
          )}
          {selectedLanguage === AUTO_TRANSLATE_CODE && translationAvailable && (
            <div className="p-2 bg-blue-50 border border-blue-200 rounded text-blue-800">
              <p className="font-medium">🌐 {t('languageSelection.translationModeTitle')}</p>
              <p className="mt-1">{t('languageSelection.translationModeBody')}</p>
            </div>
          )}
          {selectedLanguage !== 'auto' && selectedLanguage !== AUTO_TRANSLATE_CODE && (
            <p className="text-gray-600">
              {t('languageSelection.optimizedFor')} <strong>{selectedLanguageName}</strong>
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
