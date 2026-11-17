'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
import { ScrollText, RotateCcw } from 'lucide-react';

const PLACEHOLDER_HINT =
  'Available placeholders: {section_instructions} — the instructions of the selected template sections; {template} — the Markdown structure of the selected template. A template with its own system prompt replaces this one entirely.';

export function SummarySystemPromptSettings() {
  const [prompt, setPrompt] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);

  const loadPrompt = useCallback(async () => {
    try {
      const stored = await invoke<string>('api_get_summary_system_prompt');
      setPrompt(stored);
    } catch (error) {
      console.error('Failed to load summary system prompt:', error);
      toast.error('Failed to load system prompt');
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

  const handleSave = () => savePrompt(prompt, 'System prompt saved successfully', 'Failed to save system prompt');

  const handleReset = async () => {
    // Clearing the stored value makes the backend return the built-in default.
    await savePrompt('', 'System prompt reset to default', 'Failed to reset system prompt');
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
        <h3 className="text-lg font-semibold text-gray-900">Base System Prompt</h3>
      </div>
      <p className="text-sm text-gray-600 mb-4">
        Sent to the AI model with every summary request. Templates that define their own
        system prompt override this text entirely.
      </p>

      <Textarea
        aria-label="Base system prompt"
        value={prompt}
        onChange={(event) => setPrompt(event.target.value)}
        disabled={isLoading || isSaving}
        className="min-h-[220px] font-mono text-xs"
      />

      <p className="text-xs text-gray-400 mt-2">{PLACEHOLDER_HINT}</p>

      <div className="flex items-center justify-end gap-2 mt-4">
        <Button
          type="button"
          variant="outline"
          onClick={handleReset}
          disabled={isLoading || isSaving}
        >
          <RotateCcw size={14} className="mr-1" />
          Reset to default
        </Button>
        <Button
          type="button"
          onClick={handleSave}
          disabled={isLoading || isSaving}
        >
          Save prompt
        </Button>
      </div>
    </div>
  );
}
