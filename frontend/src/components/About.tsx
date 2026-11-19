import React, { useState, useEffect } from "react";
import { useTranslation } from 'react-i18next';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import Image from 'next/image';
import { UpdateDialog } from "./UpdateDialog";
import { updateService, UpdateInfo } from '@/services/updateService';
import { Button } from './ui/button';
import { Loader2, CheckCircle2, ArrowUpRight } from 'lucide-react';
import { toast } from 'sonner';


export function About() {
    const { t } = useTranslation();
    const [currentVersion, setCurrentVersion] = useState<string>('');
    const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
    const [isChecking, setIsChecking] = useState(false);
    const [showUpdateDialog, setShowUpdateDialog] = useState(false);

    useEffect(() => {
        // Get current version on mount
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    const handleCheckForUpdates = async () => {
        setIsChecking(true);
        try {
            const info = await updateService.checkForUpdates(true);
            setUpdateInfo(info);
            if (info.available) {
                setShowUpdateDialog(true);
            } else {
                toast.success(t('updates.toast.upToDate'));
            }
        } catch (error: any) {
            console.error('Failed to check for updates:', error);
            toast.error(t('updates.toast.checkFailed', { reason: error.message || t('misc.unknownError') }));
        } finally {
            setIsChecking(false);
        }
    };

    const handleOpenGitHub = async () => {
        try {
            await invoke('open_external_url', { url: 'https://github.com/timurmazitov/samobranka' });
        } catch (error: any) {
            console.error('Failed to open GitHub link:', error);
            toast.error(t('misc.openLinkFailed', { reason: error.message || t('misc.unknownError') }));
        }
    };

    return (
        <div className="p-4 space-y-4 max-h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="icon_128x128.png"
                        alt={t('misc.about.logoAlt')}
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                {/* <h1 className="text-xl font-bold text-gray-900">Samobranka</h1> */}
                {currentVersion && <span className="text-sm text-gray-500"> v{currentVersion}</span>}
                <p className="text-medium text-gray-600 mt-1">
                    {t('misc.about.tagline')}
                </p>
                <div className="mt-3">
                    <Button
                        onClick={handleCheckForUpdates}
                        disabled={isChecking}
                        variant="outline"
                        size="sm"
                        className="text-xs"
                    >
                        {isChecking ? (
                            <>
                                <Loader2 className="h-3 w-3 mr-2 animate-spin" />
                                {t('updates.checking')}
                            </>
                        ) : (
                            <>
                                <CheckCircle2 className="h-3 w-3 mr-2" />
                                {t('updates.checkForUpdates')}
                            </>
                        )}
                    </Button>
                    {updateInfo?.available && (
                        <div className="mt-2 text-xs text-blue-600">
                            {t('updates.availableShort', { version: updateInfo.version })}
                        </div>
                    )}
                </div>
                <div className="mt-3">
                    <button
                        type="button"
                        onClick={handleOpenGitHub}
                        className="inline-flex items-center gap-1 text-xs text-gray-600 hover:underline"
                    >
                        GitHub · timurmazitov/samobranka
                        <ArrowUpRight aria-hidden="true" className="h-3 w-3" />
                    </button>
                </div>
            </div>

            {/* Update Dialog */}
            <UpdateDialog
                open={showUpdateDialog}
                onOpenChange={setShowUpdateDialog}
                updateInfo={updateInfo}
            />
        </div>

    )
}