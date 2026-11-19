"use client";

import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, Download, Save, Loader2 } from 'lucide-react';
import '@/i18n';

interface SummaryUpdaterButtonGroupProps {
  isSaving: boolean;
  isDirty: boolean;
  onSave: () => Promise<void>;
  onCopy: () => Promise<void>;
  onExport: () => Promise<void>;
}

export function SummaryUpdaterButtonGroup({
  isSaving,
  isDirty,
  onSave,
  onCopy,
  onExport,
}: SummaryUpdaterButtonGroupProps) {
  const { t } = useTranslation();
  return (
    <ButtonGroup>
      {/* Save button */}
      <Button
        variant="outline"
        size="sm"
        className={`${isDirty ? 'bg-green-200' : ""}`}
        title={isSaving ? t('summary.updater.savingTitle') : t('summary.updater.saveTitle')}
        onClick={() => {
          onSave();
        }}
        disabled={isSaving}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden @[40rem]:inline">{t('summary.updater.saving')}</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden @[40rem]:inline">{t('common.save')}</span>
          </>
        )}
      </Button>

      {/* Copy button */}
      <Button
        variant="outline"
        size="sm"
        title={t('summary.updater.copyTitle')}
        onClick={() => {
          onCopy();
        }}
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden @[40rem]:inline">{t('common.copy')}</span>
      </Button>

      {/* Export button */}
      <Button
        variant="outline"
        size="sm"
        title={t('summary.updater.exportTitle')}
        onClick={() => {
          onExport();
        }}
        className="cursor-pointer"
      >
        <Download />
        <span className="hidden @[40rem]:inline">{t('summary.updater.export')}</span>
      </Button>

    </ButtonGroup>
  );
}
