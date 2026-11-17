'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Check, ChevronsUpDown, Loader2, Users } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  Command,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command';
import { DIARIZATION_UNAVAILABLE_HINT } from '@/lib/diarization';
import type { MeetingSpeaker, SpeakerAssignment, SpeakerNameEntry } from '@/types';

/** How long the name picker waits for more typing before searching. */
const DIRECTORY_DEBOUNCE_MS = 200;
/** Page size requested from the name directory. */
const DIRECTORY_LIMIT = 20;

/** `speaker_0` → `Speaker 1`; anything else is shown as-is. */
function displayLabel(label: string): string {
  const match = /^speaker_(\d+)$/.exec(label);
  return match ? `Speaker ${Number(match[1]) + 1}` : label;
}

function segmentCountLabel(count: number): string {
  return `${count} ${count === 1 ? 'segment' : 'segments'}`;
}

interface SpeakerNamePickerProps {
  label: string;
  value: string | null;
  suggestions: string[];
  disabled?: boolean;
  onValueChange: (name: string | null) => void;
}

/**
 * Popover + Command combobox for one speaker row: "No name", the (optional)
 * create row and the matching directory entries, in that order.
 */
function SpeakerNamePicker({
  label,
  value,
  suggestions,
  disabled,
  onValueChange,
}: SpeakerNamePickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<string[]>(suggestions);

  // Drop a previous search when the picker closes, so reopening starts fresh.
  useEffect(() => {
    if (!open) setQuery('');
  }, [open]);

  // The backend already ships a handful of most-used names per row, so an empty
  // query is served locally (opening a picker must not fire a request). Typing
  // searches the global directory, debounced.
  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setResults(suggestions);
      return;
    }

    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const entries = await invoke<SpeakerNameEntry[]>('api_get_speaker_names', {
          query: trimmed,
          limit: DIRECTORY_LIMIT,
        });
        if (!cancelled) setResults(entries.map((entry) => entry.name));
      } catch {
        // Keep the previous list; an autocomplete hiccup must not break the panel.
      }
    }, DIRECTORY_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query, suggestions]);

  const trimmed = query.trim();
  const lowered = trimmed.toLowerCase();
  const exactMatch = results.some((name) => name.toLowerCase() === lowered);
  const matches = lowered
    ? results.filter((name) => name.toLowerCase().includes(lowered))
    : results;
  const canCreate = trimmed.length > 0 && !exactMatch;

  const choose = (name: string | null) => {
    onValueChange(name);
    setOpen(false);
    setQuery('');
  };

  return (
    <Popover open={open} onOpenChange={setOpen} modal={true}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          role="combobox"
          aria-expanded={open}
          aria-label={`Name for ${displayLabel(label)}`}
          disabled={disabled}
          className="w-full justify-between font-normal"
        >
          <span className="truncate">{value ?? 'No name'}</span>
          <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[250px] p-0" align="start">
        <Command shouldFilter={false}>
          <CommandInput
            placeholder="Search people…"
            value={query}
            onValueChange={setQuery}
          />
          <CommandList className="max-h-[260px]">
            <CommandGroup>
              <CommandItem value="__no_name__" onSelect={() => choose(null)}>
                <Check
                  className={`mr-2 h-4 w-4 ${value === null ? 'opacity-100' : 'opacity-0'}`}
                />
                <span className="truncate">No name</span>
              </CommandItem>
              {canCreate && (
                <CommandItem value={`__create__${trimmed}`} onSelect={() => choose(trimmed)}>
                  <span className="truncate">Create „{trimmed}“</span>
                </CommandItem>
              )}
              {matches.map((name) => (
                <CommandItem key={name} value={name} onSelect={() => choose(name)}>
                  <Check
                    className={`mr-2 h-4 w-4 ${
                      value?.toLowerCase() === name.toLowerCase() ? 'opacity-100' : 'opacity-0'
                    }`}
                  />
                  <span className="truncate">{name}</span>
                </CommandItem>
              ))}
              {matches.length === 0 && !canCreate && (
                <div className="py-4 text-center text-sm text-muted-foreground">
                  No people found.
                </div>
              )}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

interface SpeakersPanelProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  meetingId?: string;
  onRefetchTranscripts?: () => Promise<void>;
}

/**
 * Right-hand sheet listing the meeting's diarized speakers. Every edit stays in
 * a local draft; Done sends one batch assignment, while Escape / outside click /
 * Cancel just throw the draft away.
 */
export function SpeakersPanel({
  open,
  onOpenChange,
  meetingId,
  onRefetchTranscripts,
}: SpeakersPanelProps) {
  const [speakers, setSpeakers] = useState<MeetingSpeaker[]>([]);
  const [draft, setDraft] = useState<Record<string, string | null>>({});
  const [isLoading, setIsLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [panelError, setPanelError] = useState<string | null>(null);

  const loadSpeakers = useCallback(async () => {
    if (!meetingId) return;
    setIsLoading(true);
    setLoadError(null);
    try {
      const panel = await invoke<MeetingSpeaker[]>('api_get_meeting_speakers', {
        meetingId,
      });
      setSpeakers(panel);
      setDraft(Object.fromEntries(panel.map((row) => [row.label, row.speaker_name ?? null])));
    } catch (error) {
      setLoadError(String(error));
      setSpeakers([]);
      setDraft({});
    } finally {
      setIsLoading(false);
    }
  }, [meetingId]);

  // Load on open only (and drop the draft when the sheet closes).
  useEffect(() => {
    if (open) {
      void loadSpeakers();
    } else {
      setDraft({});
      setRowErrors({});
      setPanelError(null);
    }
  }, [open, loadSpeakers]);

  const resetDraft = () => {
    setDraft({});
    setRowErrors({});
    setPanelError(null);
  };

  const handleOpenChange = (next: boolean) => {
    if (next) {
      onOpenChange(true);
      return;
    }
    // While a save is in flight, Escape / outside click must not close the sheet.
    if (isSaving) return;
    resetDraft();
    onOpenChange(false);
  };

  const setRowValue = (label: string, name: string | null) => {
    setDraft((previous) => ({ ...previous, [label]: name }));
    setRowErrors((previous) => {
      if (!previous[label]) return previous;
      const { [label]: _dropped, ...rest } = previous;
      return rest;
    });
  };

  const handleDone = async () => {
    if (!meetingId) return;

    const assignments: SpeakerAssignment[] = speakers
      .filter((row) => (draft[row.label] ?? null) !== (row.speaker_name ?? null))
      .map((row) => ({ label: row.label, name: draft[row.label] ?? null }));

    if (assignments.length === 0) {
      handleOpenChange(false);
      return;
    }

    setIsSaving(true);
    setRowErrors({});
    setPanelError(null);
    try {
      await invoke<MeetingSpeaker[]>('api_assign_meeting_speakers', {
        meetingId,
        assignments,
      });
      toast.success('Speaker names saved');
      // Close directly: `handleOpenChange` blocks while a save is in flight.
      resetDraft();
      onOpenChange(false);
      await onRefetchTranscripts?.();
      // Ticket 06's per-row dropdown and the page-level speaker list listen for
      // this so the transcript and dropdowns pick up the new names.
      if (typeof window !== 'undefined' && typeof window.dispatchEvent === 'function') {
        window.dispatchEvent(new CustomEvent('speakers-updated', { detail: { meetingId } }));
      }
    } catch (error) {
      const message = String(error);
      setPanelError(null);
      const conflict = /Speaker (\d+)/.exec(message);
      const label = conflict
        ? `speaker_${Number(conflict[1]) - 1}`
        : null;
      if (label && speakers.some((row) => row.label === label)) {
        setRowErrors({ [label]: message });
      } else {
        setPanelError(message);
      }
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Sheet open={open} onOpenChange={handleOpenChange}>
      <SheetContent side="right" className="flex flex-col gap-4 sm:max-w-md">
        <SheetHeader>
          <SheetTitle className="flex items-center gap-2">
            <Users className="h-5 w-5" />
            Speakers
          </SheetTitle>
          <SheetDescription>
            Give this meeting&apos;s speakers a name. Names are reused across meetings.
          </SheetDescription>
        </SheetHeader>

        <div className="flex-1 overflow-y-auto">
          {isLoading ? (
            <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              Loading speakers…
            </div>
          ) : loadError ? (
            <div className="py-6 text-center text-sm text-red-600">{loadError}</div>
          ) : speakers.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">
              {DIARIZATION_UNAVAILABLE_HINT}
            </p>
          ) : (
            <ul className="space-y-4">
              {speakers.map((row) => (
                <li
                  key={row.label}
                  data-testid={`speaker-row-${row.label}`}
                  className="space-y-1"
                >
                  <div className="flex items-baseline justify-between gap-2">
                    <span className="text-sm font-medium">{displayLabel(row.label)}</span>
                    <span className="text-xs text-muted-foreground">
                      {segmentCountLabel(row.segment_count)}
                    </span>
                  </div>
                  <SpeakerNamePicker
                    label={row.label}
                    value={draft[row.label] ?? null}
                    suggestions={row.top_speaker_names}
                    disabled={isSaving}
                    onValueChange={(name) => setRowValue(row.label, name)}
                  />
                  {rowErrors[row.label] && (
                    <p className="text-xs text-red-600">{rowErrors[row.label]}</p>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        {panelError && <p className="text-sm text-red-600">{panelError}</p>}

        <SheetFooter className="gap-2">
          <Button variant="outline" onClick={() => handleOpenChange(false)} disabled={isSaving}>
            Cancel
          </Button>
          <Button onClick={handleDone} disabled={isSaving || isLoading}>
            {isSaving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Done
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}
