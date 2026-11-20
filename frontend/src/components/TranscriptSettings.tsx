import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { AudioCppModelManager } from './AudioCppModelManager';
import { TranscribeCppModelManager } from './TranscribeCppModelManager';
import { modelForProviderSwitch } from '@/lib/transcription-provider';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'audioCpp' | 'transcribeCpp' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
    /**
     * Transcribe.cpp compute mode («режим вычислений»): 'auto' | 'vulkan' |
     * 'cpu'. Only meaningful for the transcribeCpp provider; absent means
     * 'auto' (the backend column's schema default).
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
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet' || transcriptModelConfig.provider === 'audioCpp' || transcriptModelConfig.provider === 'transcribeCpp') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        audioCpp: [], // Model selection handled by AudioCppModelManager component
        transcribeCpp: [], // Model selection handled by TranscribeCppModelManager component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleWhisperModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'localWhisper', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    const handleParakeetModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'parakeet', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    const handleAudioCppModelSelect = (modelName: string) => {
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'audioCpp',
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

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

    /**
     * Persist a provider change immediately.
     *
     * Selecting a provider is a configuration change in its own right: without
     * saving here the choice would be lost on restart unless the user also
     * clicked a model. Local providers need no API key.
     */
    const handleProviderChange = async (provider: TranscriptModelProps['provider']) => {
        setUiProvider(provider);
        if (provider === 'localWhisper' || provider === 'parakeet' || provider === 'audioCpp' || provider === 'transcribeCpp') {
            // Catalogue-backed: the model is chosen in the manager below.
            const model = modelForProviderSwitch(
                provider,
                transcriptModelConfig.provider,
                transcriptModelConfig.model,
            );
            setTranscriptModelConfig({ ...transcriptModelConfig, provider, model });
            try {
                if (provider === 'transcribeCpp') {
                    // Carry the saved compute mode so a provider switch does
                    // not reset it; the backend normalizes absent → 'auto'.
                    await invoke('api_save_transcript_config', {
                        provider,
                        model,
                        apiKey: null,
                        transcribeBackend: transcriptModelConfig.transcribeBackend ?? 'auto',
                    });
                } else {
                    await invoke('api_save_transcript_config', { provider, model, apiKey: null });
                }
            } catch (error) {
                console.error('Failed to save transcript provider:', error);
            }
            return;
        }
        fetchApiKey(provider);
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            {t('settingsTranscription.model')}
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                onValueChange={(value) => {
                                    void handleProviderChange(value as TranscriptModelProps['provider']);
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder={t('models.common.selectProvider')} />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">{t('settingsTranscription.providerParakeet')}</SelectItem>
                                    <SelectItem value="localWhisper">{t('settingsTranscription.providerWhisper')}</SelectItem>
                                    <SelectItem value="audioCpp">{t('settingsTranscription.providerAudioCpp')}</SelectItem>
                                    <SelectItem value="transcribeCpp">{t('settingsTranscription.providerTranscribeCpp')}</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'audioCpp' && uiProvider !== 'transcribeCpp' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder={t('settingsTranscription.selectModel')} />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[uiProvider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {uiProvider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'audioCpp' && (
                        <div className="mt-6">
                            <AudioCppModelManager
                                selectedModel={transcriptModelConfig.provider === 'audioCpp' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleAudioCppModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'transcribeCpp' && (
                        <div className="mt-6">
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
                                selectedModel={transcriptModelConfig.provider === 'transcribeCpp' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleTranscribeCppModelSelect}
                                saveSelection={saveTranscribeCppSelection}
                                autoSave={true}
                            />
                        </div>
                    )}

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
                            <AudioCppModelManager task="diar" />
                        </div>
                    </div>


                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                {t('models.common.apiKey')}
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => setApiKey(e.target.value)}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder={t('models.common.enterApiKey')}
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? t('models.common.unlockToEdit') : t('models.common.lockToPreventEditing')}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div >
    )
}








