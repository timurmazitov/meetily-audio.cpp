"use client"

import { useEffect, useState } from "react"
import { Switch } from "./ui/switch"
import { FolderOpen, FolderCog } from "lucide-react"
import { invoke } from "@tauri-apps/api/core"
import { toast } from "sonner"
import { useConfig, NotificationSettings } from "@/contexts/ConfigContext"

interface SelectedRecordingsFolder {
  path: string;
  free_bytes: number;
}

// Warn (not block) when the target disk has less than this much free space.
const LOW_SPACE_THRESHOLD_BYTES = 1 * 1024 * 1024 * 1024; // 1 GB

export function PreferenceSettings() {
  const {
    notificationSettings,
    storageLocations,
    isLoadingPreferences,
    loadPreferences,
    refreshStorageLocations,
    updateNotificationSettings
  } = useConfig();

  const [notificationsEnabled, setNotificationsEnabled] = useState<boolean | null>(null);
  const [isInitialLoad, setIsInitialLoad] = useState(true);
  const [previousNotificationsEnabled, setPreviousNotificationsEnabled] = useState<boolean | null>(null);
  const [isChoosingFolder, setIsChoosingFolder] = useState(false);

  // Lazy load preferences on mount (only loads if not already cached)
  useEffect(() => {
    loadPreferences();
  }, [loadPreferences]);

  // Update notificationsEnabled when notificationSettings are loaded from global state
  useEffect(() => {
    if (notificationSettings) {
      // Notification enabled means both started and stopped notifications are enabled
      const enabled =
        notificationSettings.notification_preferences.show_recording_started &&
        notificationSettings.notification_preferences.show_recording_stopped;
      setNotificationsEnabled(enabled);
      if (isInitialLoad) {
        setPreviousNotificationsEnabled(enabled);
        setIsInitialLoad(false);
      }
    } else if (!isLoadingPreferences) {
      // If not loading and no settings, use default
      setNotificationsEnabled(true);
      if (isInitialLoad) {
        setPreviousNotificationsEnabled(true);
        setIsInitialLoad(false);
      }
    }
  }, [notificationSettings, isLoadingPreferences, isInitialLoad])

  useEffect(() => {
    // Skip update on initial load or if value hasn't actually changed
    if (isInitialLoad || notificationsEnabled === null || notificationsEnabled === previousNotificationsEnabled) return;
    if (!notificationSettings) return;

    const handleUpdateNotificationSettings = async () => {
      console.log("Updating notification settings to:", notificationsEnabled);

      try {
        // Update the notification preferences
        const updatedSettings: NotificationSettings = {
          ...notificationSettings,
          notification_preferences: {
            ...notificationSettings.notification_preferences,
            show_recording_started: notificationsEnabled,
            show_recording_stopped: notificationsEnabled,
          }
        };

        console.log("Calling updateNotificationSettings with:", updatedSettings);
        await updateNotificationSettings(updatedSettings);
        setPreviousNotificationsEnabled(notificationsEnabled);
        console.log("Successfully updated notification settings to:", notificationsEnabled);
      } catch (error) {
        console.error('Failed to update notification settings:', error);
      }
    };

    handleUpdateNotificationSettings();
  }, [notificationsEnabled, notificationSettings, isInitialLoad, previousNotificationsEnabled, updateNotificationSettings])

  const handleOpenFolder = async (folderType: 'models' | 'recordings') => {
    try {
      switch (folderType) {
        case 'models':
          await invoke('open_models_folder');
          break;
        case 'recordings':
          await invoke('open_recordings_folder');
          break;
      }
    } catch (error) {
      console.error(`Failed to open ${folderType} folder:`, error);
    }
  };

  const handleChooseRecordingsFolder = async () => {
    if (isChoosingFolder) return;
    setIsChoosingFolder(true);
    try {
      const selected = await invoke<SelectedRecordingsFolder | null>('select_recording_folder');
      if (!selected) return; // user cancelled

      const prefs = await invoke<{ save_folder: string } & Record<string, unknown>>('get_recording_preferences');
      await invoke('set_recording_preferences', {
        preferences: { ...prefs, save_folder: selected.path }
      });
      await refreshStorageLocations();

      if (selected.free_bytes > 0 && selected.free_bytes < LOW_SPACE_THRESHOLD_BYTES) {
        toast.warning('Low disk space', {
          description: `Less than 1 GB free on the selected drive. New recordings are saved there.`,
        });
      } else {
        toast.success('Recordings folder updated', {
          description: 'Applies to new recordings. Existing recordings stay in their current locations.',
        });
      }
    } catch (error) {
      console.error('Failed to choose recordings folder:', error);
      toast.error('Failed to update recordings folder', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsChoosingFolder(false);
    }
  };

  // Show loading only if we're actually loading and don't have cached data
  if (isLoadingPreferences && !notificationSettings && !storageLocations) {
    return <div className="max-w-2xl mx-auto p-6">Loading Preferences...</div>
  }

  // Show loading if notificationsEnabled hasn't been determined yet
  if (notificationsEnabled === null && !isLoadingPreferences) {
    return <div className="max-w-2xl mx-auto p-6">Loading Preferences...</div>
  }

  // Ensure we have a boolean value for the Switch component
  const notificationsEnabledValue = notificationsEnabled ?? false;

  return (
    <div className="space-y-6">
      {/* Notifications Section */}
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-lg font-semibold text-gray-900 mb-2">Notifications</h3>
            <p className="text-sm text-gray-600">Enable or disable notifications of start and end of meeting</p>
          </div>
          <Switch checked={notificationsEnabledValue} onCheckedChange={setNotificationsEnabled} />
        </div>
      </div>

      {/* Data Storage Locations Section */}
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Data Storage Locations</h3>
        <p className="text-sm text-gray-600 mb-6">
          View and access where Samobranka stores your data
        </p>

        <div className="space-y-4">
          {/* Models Location (informational — derived from the app data directory) */}
          <div className="p-4 border rounded-lg bg-gray-50">
            <div className="font-medium mb-2">Models</div>
            <div className="text-sm text-gray-600 mb-3 break-all font-mono text-xs">
              {storageLocations?.models || 'Loading...'}
            </div>
            <button
              onClick={() => handleOpenFolder('models')}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div>

          {/* Recordings Location */}
          <div className="p-4 border rounded-lg bg-gray-50">
            <div className="font-medium mb-2">Meeting Recordings</div>
            <div className="text-sm text-gray-600 mb-3 break-all font-mono text-xs">
              {storageLocations?.recordings || 'Loading...'}
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={handleChooseRecordingsFolder}
                disabled={isChoosingFolder}
                className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors disabled:opacity-50"
              >
                <FolderCog className="w-4 h-4" />
                {isChoosingFolder ? 'Choosing...' : 'Choose...'}
              </button>
              <button
                onClick={() => handleOpenFolder('recordings')}
                className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
              >
                <FolderOpen className="w-4 h-4" />
                Open Folder
              </button>
            </div>
            <p className="text-xs text-gray-500 mt-2">
              Applies to new recordings. Existing recordings stay in their current locations.
            </p>
          </div>
        </div>

        <div className="mt-4 p-3 bg-blue-50 rounded-md">
          <p className="text-xs text-blue-800">
            <strong>Note:</strong> Database and models are stored together in your application data directory for unified management.
          </p>
        </div>
      </div>
    </div>
  )
}
