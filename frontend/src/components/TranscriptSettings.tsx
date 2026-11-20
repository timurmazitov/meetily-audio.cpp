import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Label } from './ui/label';
import { useTranslation } from 'react-i18next';
import { AudioCppModelManager } from './AudioCppModelManager';
import { TranscribeCppModelManager } from './TranscribeCppModelManager';


export interface TranscriptModelProps {
    /**
     * transcribe.cpp is the only local transcription provider; the settings
     * repository soft-migrates the removed `localWhisper`/`parakeet`/`audioCpp`
     * values on read. Cloud providers (deepgram, elevenLabs, groq, openai) are
     * schema-live but have no UI today.
     */
    provider: 'transcribeCpp';
    model: string;
    apiKey?: string | null;
    /**
     * Transcribe.cpp compute mode («режим вычислений»): 'auto' | 'vulkan' |
     * 'cpu'. Absent means 'auto' (the backend column's schema default).
     */
    transcribeBackend?: 'auto' | 'vulkan' | 'cpu';
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const { t } = useTranslation();

    const handleTranscribeCppModelSelect = (modelName: string) => {
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'transcribeCpp',
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    /**
     * Persist a model selection with the current compute mode attached.
     *
     * The manager saves the provider/model pair itself; without the mode in
     * that payload, picking a model would reset the saved «режим вычислений»
     * to its default. Threading it through keeps the mode stable across model
     * changes (the backend treats an absent value as 'auto').
     */
    const saveTranscribeCppSelection = async (config: {
        provider: string;
        model: string;
        apiKey: null;
    }) => {
        await invoke('api_save_transcript_config', {
            ...config,
            transcribeBackend: transcriptModelConfig.transcribeBackend ?? 'auto',
        });
    };

    /**
     * Persist a compute-mode change immediately, alongside the current
     * provider/model. Selecting the mode is a configuration change in its own
     * right: saving here makes it survive a restart without the user also
     * clicking a model.
     */
    const handleTranscribeBackendChange = async (transcribeBackend: 'auto' | 'vulkan' | 'cpu') => {
        setTranscriptModelConfig({ ...transcriptModelConfig, transcribeBackend });
        try {
            await invoke('api_save_transcript_config', {
                provider: transcriptModelConfig.provider,
                model: transcriptModelConfig.model,
                apiKey: null,
                transcribeBackend,
            });
        } catch (error) {
            console.error('Failed to save transcribe.cpp compute mode:', error);
        }
    };

    return (
        <div>
            <div>
                <div className="space-y-4 pb-6">
                    <div className="mb-3">
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            {t('settingsTranscription.transcribeBackendLabel')}
                        </Label>
                        <Select
                            value={transcriptModelConfig.transcribeBackend ?? 'auto'}
                            onValueChange={(value) => {
                                void handleTranscribeBackendChange(
                                    value as 'auto' | 'vulkan' | 'cpu',
                                );
                            }}
                        >
                            <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500' data-testid="transcribe-backend-select">
                                <SelectValue placeholder={t('settingsTranscription.transcribeBackendLabel')} />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="auto">{t('settingsTranscription.transcribeBackendAuto')}</SelectItem>
                                <SelectItem value="vulkan">{t('settingsTranscription.transcribeBackendVulkan')}</SelectItem>
                                <SelectItem value="cpu">{t('settingsTranscription.transcribeBackendCpu')}</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>
                    <TranscribeCppModelManager
                        selectedModel={transcriptModelConfig.model}
                        onModelSelect={handleTranscribeCppModelSelect}
                        saveSelection={saveTranscribeCppSelection}
                        autoSave={true}
                    />

                    {/*
                     * Speaker diarization is a second stage that runs over a
                     * finished transcript, so its model is offered whatever
                     * engine produced the text. No selection is passed: a
                     * diarization model cannot transcribe.
                     */}
                    <div className="mt-6 border-t border-gray-200 pt-4">
                        <Label className="block text-sm font-medium text-gray-700">
                            {t('settingsTranscription.diarization.title')}
                        </Label>
                        <p className="mt-1 text-xs text-gray-500">
                            {t('settingsTranscription.diarization.description')}
                        </p>
                        <div className="mt-3">
                            <AudioCppModelManager />
                        </div>
                    </div>
                </div>
            </div>
        </div >
    )
}
