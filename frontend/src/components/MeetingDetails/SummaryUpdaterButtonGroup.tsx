"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, Download, Save, Loader2 } from 'lucide-react';

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
  return (
    <ButtonGroup>
      {/* Save button */}
      <Button
        variant="outline"
        size="sm"
        className={`${isDirty ? 'bg-green-200' : ""}`}
        title={isSaving ? "Saving" : "Save Changes"}
        onClick={() => {
          onSave();
        }}
        disabled={isSaving}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden @[40rem]:inline">Saving...</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden @[40rem]:inline">Save</span>
          </>
        )}
      </Button>

      {/* Copy button */}
      <Button
        variant="outline"
        size="sm"
        title="Copy Summary"
        onClick={() => {
          onCopy();
        }}
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden @[40rem]:inline">Copy</span>
      </Button>

      {/* Export button */}
      <Button
        variant="outline"
        size="sm"
        title="Export Summary"
        onClick={() => {
          onExport();
        }}
        className="cursor-pointer"
      >
        <Download />
        <span className="hidden @[40rem]:inline">Export</span>
      </Button>

    </ButtonGroup>
  );
}
