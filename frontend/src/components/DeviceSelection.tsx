import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw, Mic, Speaker, CircleHelp } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { AudioLevelMeter, CompactAudioLevelMeter } from './AudioLevelMeter';
import { AudioBackendSelector } from './AudioBackendSelector';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  AudioDevice,
  SelectedDevices,
  decodeDeviceOption,
  deviceLabel,
  encodeDeviceOption,
  fetchAudioDevices,
  isDeviceAvailable,
  cachedDeviceLabel,
} from '@/lib/audioDevices';

export type { AudioDevice, SelectedDevices };

export interface AudioLevelData {
  device_name: string;
  device_type: string;
  rms_level: number;
  peak_level: number;
  is_active: boolean;
}

export interface AudioLevelUpdate {
  timestamp: number;
  levels: AudioLevelData[];
}

interface DeviceSelectionProps {
  selectedDevices: SelectedDevices;
  onDeviceChange: (devices: SelectedDevices) => void;
  disabled?: boolean;
}

export function DeviceSelection({ selectedDevices, onDeviceChange, disabled = false }: DeviceSelectionProps) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [audioLevels, setAudioLevels] = useState<Map<string, AudioLevelData>>(new Map());
  const [isMonitoring, setIsMonitoring] = useState(false);
  const [showLevels, setShowLevels] = useState(false);

  // Filter devices by type
  const inputDevices = devices.filter(device => device.device_type === 'Input');
  const outputDevices = devices.filter(device => device.device_type === 'Output');

  // Fetch available audio devices
  const fetchDevices = async () => {
    try {
      setError(null);
      const result = await fetchAudioDevices();
      setDevices(result);
      console.log('Fetched audio devices:', result);
    } catch (err) {
      console.error('Failed to fetch audio devices:', err);
      setError(t('devices.loadFailed'));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  // Load devices on component mount
  useEffect(() => {
    fetchDevices();
  }, []);

  // Set up audio level event listener
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupAudioLevelListener = async () => {
      try {
        unlisten = await listen<AudioLevelUpdate>('audio-levels', (event) => {
          const levelUpdate = event.payload;
          const newLevels = new Map<string, AudioLevelData>();

          levelUpdate.levels.forEach(level => {
            newLevels.set(level.device_name, level);
          });

          setAudioLevels(newLevels);
        });
      } catch (err) {
        console.error('Failed to setup audio level listener:', err);
      }
    };

    setupAudioLevelListener();

    // Cleanup function
    return () => {
      if (unlisten) {
        unlisten();
      }
      // Stop monitoring when component unmounts
      if (isMonitoring) {
        stopAudioLevelMonitoring();
      }
    };
  }, [isMonitoring]);

  // Handle device refresh
  const handleRefresh = async () => {
    setRefreshing(true);
    await fetchDevices();
  };

  // Handle microphone device selection
  const handleMicDeviceChange = (optionValue: string) => {
    const deviceName = decodeDeviceOption(optionValue);
    if (deviceName === undefined) return;
    const newDevices = {
      ...selectedDevices,
      micDevice: deviceName
    };
    onDeviceChange(newDevices);
  };

  // Handle system audio device selection
  const handleSystemDeviceChange = (optionValue: string) => {
    const deviceName = decodeDeviceOption(optionValue);
    if (deviceName === undefined) return;
    const newDevices = {
      ...selectedDevices,
      systemDevice: deviceName
    };
    onDeviceChange(newDevices);
  };

  // Start audio level monitoring
  const startAudioLevelMonitoring = async () => {
    try {
      // Only monitor input devices for now (microphones)
      const deviceNames = inputDevices.map(device => device.name);
      if (deviceNames.length === 0) {
        setError(t('devices.noMicrophonesToMonitor'));
        return;
      }

      await invoke('start_audio_level_monitoring', { deviceNames });
      setIsMonitoring(true);
      setShowLevels(true);
      console.log('Started audio level monitoring for input devices:', deviceNames);
    } catch (err) {
      console.error('Failed to start audio level monitoring:', err);
      setError(t('devices.monitoringStartFailed'));
    }
  };

  // Stop audio level monitoring
  const stopAudioLevelMonitoring = async () => {
    try {
      await invoke('stop_audio_level_monitoring');
      setIsMonitoring(false);
      setAudioLevels(new Map());
      console.log('Stopped audio level monitoring');
    } catch (err) {
      console.error('Failed to stop audio level monitoring:', err);
    }
  };

  // Toggle audio level monitoring
  const toggleAudioLevelMonitoring = async () => {
    if (isMonitoring) {
      await stopAudioLevelMonitoring();
    } else {
      await startAudioLevelMonitoring();
    }
  };

  if (loading) {
    return (
      <div className="p-4 space-y-4">
        <div className="animate-pulse">
          <div className="h-4 bg-gray-200 rounded w-1/3 mb-4"></div>
          <div className="h-10 bg-gray-200 rounded mb-3"></div>
          <div className="h-10 bg-gray-200 rounded"></div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-gray-900">{t('devices.title')}</h4>
        <div className="flex items-center space-x-2">
          {/* TODO: Monitoring */}
          {/* <button */}
          {/*   onClick={toggleAudioLevelMonitoring} */}
          {/*   disabled={disabled || inputDevices.length === 0} */}
          {/*   className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${ */}
          {/*     isMonitoring */}
          {/*       ? 'bg-red-100 text-red-700 hover:bg-red-200' */}
          {/*       : 'bg-green-100 text-green-700 hover:bg-green-200' */}
          {/*   } disabled:pointer-events-none disabled:opacity-50`} */}
          {/*   title={inputDevices.length === 0 ? 'No microphones available to test' : ''} */}
          {/* > */}
          {/*   {isMonitoring ? 'Stop Test' : 'Test Mic'} */}
          {/* </button> */}
          <button
            onClick={handleRefresh}
            disabled={refreshing || disabled}
            className="h-8 w-8 p-0 inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors hover:bg-gray-100 disabled:pointer-events-none disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {error && (
        <div className="p-3 text-sm text-red-700 bg-red-50 border border-red-200 rounded-md">
          {error}
        </div>
      )}

      <div className="space-y-3">
        {/* Microphone Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Mic className="h-4 w-4 text-gray-600" />
            <Label htmlFor="mic-selection" className="text-sm font-medium text-gray-700">
              {t('devices.microphone')}
            </Label>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label={t('devices.microphoneAria')}
                  className="text-gray-400 hover:text-gray-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 rounded-full"
                >
                  <CircleHelp aria-hidden="true" className="h-4 w-4" />
                </button>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs">
                {t('devices.microphoneTooltip')}
              </TooltipContent>
            </Tooltip>
          </div>
          <Select
            value={encodeDeviceOption(selectedDevices.micDevice)}
            onValueChange={handleMicDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="mic-selection" className="w-full">
              <SelectValue placeholder={t('devices.selectMicrophone')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={encodeDeviceOption(null)}>{t('devices.defaultMicrophone')}</SelectItem>
              {selectedDevices.micDevice &&
                !isDeviceAvailable(devices, 'Input', selectedDevices.micDevice) && (
                  <SelectItem value={encodeDeviceOption(selectedDevices.micDevice)}>
                    {t('devices.unavailableDevice', { name: cachedDeviceLabel(selectedDevices.micDevice) })}
                  </SelectItem>
                )}
              {inputDevices.map((device) => (
                <SelectItem
                  key={device.name}
                  value={encodeDeviceOption(device.name)}
                >
                  {deviceLabel(device)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {inputDevices.length === 0 && (
            <p className="text-xs text-gray-500">{t('devices.noMicrophones')}</p>
          )}

          {/* Audio Level Meters for Input Devices */}
          {showLevels && inputDevices.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-gray-100">
              <p className="text-xs text-gray-600 font-medium">{t('devices.microphoneLevels')}</p>
              {inputDevices.map((device) => {
                const levelData = audioLevels.get(device.name);
                return (
                  <div key={`level-${device.name}`} className="space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-gray-600 truncate max-w-[200px]">
                        {deviceLabel(device)}
                      </span>
                      {levelData && (
                        <CompactAudioLevelMeter
                          rmsLevel={levelData.rms_level}
                          peakLevel={levelData.peak_level}
                          isActive={levelData.is_active}
                        />
                      )}
                    </div>
                    {levelData && (
                      <AudioLevelMeter
                        rmsLevel={levelData.rms_level}
                        peakLevel={levelData.peak_level}
                        isActive={levelData.is_active}
                        deviceName={deviceLabel(device)}
                        size="small"
                      />
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* System Audio Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Speaker className="h-4 w-4 text-gray-600" />
            <Label htmlFor="system-selection" className="text-sm font-medium text-gray-700">
              {t('devices.systemAudio')}
            </Label>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label={t('devices.systemAudioAria')}
                  className="text-gray-400 hover:text-gray-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 rounded-full"
                >
                  <CircleHelp aria-hidden="true" className="h-4 w-4" />
                </button>
              </TooltipTrigger>
              <TooltipContent className="max-w-xs">
                {t('devices.systemAudioTooltip')}
              </TooltipContent>
            </Tooltip>
          </div>

          <Select
            value={encodeDeviceOption(selectedDevices.systemDevice)}
            onValueChange={handleSystemDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="system-selection" className="w-full">
              <SelectValue placeholder={t('devices.selectSystemAudio')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={encodeDeviceOption(null)}>{t('devices.defaultSystemAudio')}</SelectItem>
              {selectedDevices.systemDevice &&
                !isDeviceAvailable(devices, 'Output', selectedDevices.systemDevice) && (
                  <SelectItem value={encodeDeviceOption(selectedDevices.systemDevice)}>
                    {t('devices.unavailableDevice', { name: cachedDeviceLabel(selectedDevices.systemDevice) })}
                  </SelectItem>
                )}
              {outputDevices.map((device) => (
                <SelectItem
                  key={device.name}
                  value={encodeDeviceOption(device.name)}
                >
                  {deviceLabel(device)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {outputDevices.length === 0 && (
            <p className="text-xs text-gray-500">{t('devices.noSystemAudio')}</p>
          )}

          {/* Backend Selection - available on all platforms */}
          {!disabled && (
            <div className="pt-3 border-t border-gray-100">
              <AudioBackendSelector disabled={disabled} />
            </div>
          )}
        </div>
      </div>

      {/* Info text */}
      <div className="text-xs text-gray-500 space-y-1">
        <p>• <strong>{t('devices.microphone')}:</strong> {t('devices.microphoneInfo')}</p>
        <p>• <strong>{t('devices.systemAudio')}:</strong> {t('devices.systemAudioInfo')}</p>
      </div>
    </div>
  );
}
