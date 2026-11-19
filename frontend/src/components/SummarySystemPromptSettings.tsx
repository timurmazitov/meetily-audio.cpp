'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
import { ScrollText, RotateCcw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

export function SummarySystemPromptSettings() {
  const { t } = useTranslation();
  const [prompt, setPrompt] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);

  const loadPrompt = useCallback(async () => {
    try {
      const stored = await invoke<string>('api_get_summary_system_prompt');
      setPrompt(stored);
    } catch (error) {
      console.error('Failed to load summary system prompt:', error);
      toast.error(t('settingsSummary.systemPrompt.loadFailed'));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadPrompt();
  }, [loadPrompt]);

  const savePrompt = async (value: string, successMessage: string, failureMessage: string) => {
    setIsSaving(true);
    try {
      await invoke('api_save_summary_system_prompt', { prompt: value });
      toast.success(successMessage);
    } catch (error) {
      console.error('Error saving summary system prompt:', error);
      toast.error(failureMessage);
    } finally {
      setIsSaving(false);
    }
  };

  const handleSave = () => savePrompt(
    prompt,
    t('settingsSummary.systemPrompt.saved'),
    t('settingsSummary.systemPrompt.saveFailed'),
  );

  const handleReset = async () => {
    // Clearing the stored value makes the backend return the built-in default.
    await savePrompt(
      '',
      t('settingsSummary.systemPrompt.reset'),
      t('settingsSummary.systemPrompt.resetFailed'),
    );
    try {
      setPrompt(await invoke<string>('api_get_summary_system_prompt'));
    } catch (error) {
      console.error('Failed to load default summary system prompt:', error);
    }
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center gap-2 mb-2">
        <ScrollText size={18} className="text-gray-500" />
        <h3 className="text-lg font-semibold text-gray-900">{t('settingsSummary.systemPrompt.title')}</h3>
      </div>
      <p className="text-sm text-gray-600 mb-4">
        {t('settingsSummary.systemPrompt.description')}
      </p>

      <Textarea
        aria-label={t('settingsSummary.systemPrompt.ariaLabel')}
        value={prompt}
        onChange={(event) => setPrompt(event.target.value)}
        disabled={isLoading || isSaving}
        className="min-h-[220px] font-mono text-xs"
      />

      <p className="text-xs text-gray-400 mt-2">{t('settingsSummary.systemPrompt.placeholderHint')}</p>

      <div className="flex items-center justify-end gap-2 mt-4">
        <Button
          type="button"
          variant="outline"
          onClick={handleReset}
          disabled={isLoading || isSaving}
        >
          <RotateCcw size={14} className="mr-1" />
          {t('settingsSummary.systemPrompt.resetButton')}
        </Button>
        <Button
          type="button"
          onClick={handleSave}
          disabled={isLoading || isSaving}
        >
          {t('settingsSummary.systemPrompt.saveButton')}
        </Button>
      </div>
    </div>
  );
}
