'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Check, ChevronsUpDown, Loader2, Sparkles, Users } from 'lucide-react';
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
import {
  DIARIZATION_NO_SPEAKERS_FOUND_MESSAGE,
  DIARIZATION_UNAVAILABLE_HINT,
  diarizationStatusMessage,
  hasInstalledDiarizationModel,
  type DiarizationClient,
} from '@/lib/diarization';
import type { AudioCppModelClient } from '@/lib/audiocpp';
import { audioCppClient } from '@/lib/audiocppClient';
import { diarizationClient as defaultDiarizationClient } from '@/lib/diarizationClient';
import { formatSpeakerDisplayLabel } from '@/lib/transcript-format';
import type { MeetingSpeaker, SpeakerAssignment, SpeakerNameEntry } from '@/types';

/** How long the name picker waits for more typing before searching. */
const DIRECTORY_DEBOUNCE_MS = 200;
/** Page size requested from the name directory. */
const DIRECTORY_LIMIT = 20;

/** `speaker_0` → `Speaker 1`; anything else is shown as-is. */
function displayLabel(label: string): string {
  return formatSpeakerDisplayLabel(label, undefined) ?? label;
}

/** Names are compared the way the backend compares them: trimmed, case-folded. */
function foldName(name: string | null | undefined): string | null {
  const folded = name?.trim().toLowerCase();
  return folded ? folded : null;
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
  /** Diarization command seam; production uses the Tauri client. */
  diarizationClient?: DiarizationClient;
  /** Model catalog seam: decides whether the manual run button is enabled. */
  modelClient?: Pick<AudioCppModelClient, 'getAvailableModels'>;
}

/**
 * Right-hand sheet listing the meeting's diarized speakers. Every edit stays in
 * a local draft; Done sends one batch assignment, while Escape / outside click /
 * Cancel just throw the draft away.
 *
 * When the meeting has no labels the panel is no longer a dead end: it offers a
 * manual diarization run, which also covers meetings recorded before the model
 * was downloaded. The run is independent of the transcription provider.
 */
export function SpeakersPanel({
  open,
  onOpenChange,
  meetingId,
  onRefetchTranscripts,
  diarizationClient = defaultDiarizationClient,
  modelClient = audioCppClient,
}: SpeakersPanelProps) {
  const [speakers, setSpeakers] = useState<MeetingSpeaker[]>([]);
  const [draft, setDraft] = useState<Record<string, string | null>>({});
  const [isLoading, setIsLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [panelError, setPanelError] = useState<string | null>(null);
  // Starts disabled until the catalog answers, so a click can never start a
  // pass that is known to answer `no_model`.
  const [hasDiarizationModel, setHasDiarizationModel] = useState(false);
  const [isDiarizing, setIsDiarizing] = useState(false);
  const [diarizationReason, setDiarizationReason] = useState<string | null>(null);
  // Guards against a second click landing before the disabled state renders.
  const diarizingRef = useRef(false);

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
      setDiarizationReason(null);
    }
  }, [open, loadSpeakers]);

  // Whether a manual run can work at all: an installed diarization model. The
  // catalog is the same one the settings section lists, so a downloaded model is
  // immediately usable here.
  useEffect(() => {
    if (!open) return;

    let cancelled = false;
    const loadModels = async () => {
      try {
        const models = await modelClient.getAvailableModels();
        if (!cancelled) setHasDiarizationModel(hasInstalledDiarizationModel(models));
      } catch (error) {
        // An unreachable catalog is not a reason to hide the button; a run will
        // report the reason instead.
        console.warn('Failed to read the audio.cpp model list:', error);
        if (!cancelled) setHasDiarizationModel(false);
      }
    };

    void loadModels();
    return () => {
      cancelled = true;
    };
  }, [open, modelClient]);

  /**
   * Runs one diarization pass for this meeting and reports the outcome inside
   * the panel: the user pressed a button, so every refusal is shown, unlike the
   * silent automatic path.
   */
  const handleDiarize = async () => {
    // A ref guards against a second click landing before the disabled state
    // renders: two concurrent passes are exactly what native code serializes,
    // and the panel must not queue them either.
    if (!meetingId || isDiarizing || !hasDiarizationModel || diarizingRef.current) return;

    diarizingRef.current = true;
    setIsDiarizing(true);
    setDiarizationReason(null);
    setPanelError(null);
    try {
      let result;
      try {
        result = await diarizationClient.diarizeMeeting(meetingId);
      } catch (error) {
        setDiarizationReason(String(error));
        return;
      }

      const reason = diarizationStatusMessage(result.status);
      // A failed pass carries diagnostic detail; the panel shows it next to the
      // generic message so a support report can quote something concrete.
      const detail = result.status === 'failed' ? result.error : null;
      setDiarizationReason(detail ? `${reason} (${detail})` : reason);

      if (result.status === 'ran') {
        // Labels (and possibly names) changed: reload both the panel's own list
        // and the transcript behind the sheet.
        await loadSpeakers();
        await onRefetchTranscripts?.();
      }
      if (result.status === 'ran' && result.labeled_segments === 0) {
        // A pass that found no distinct voices is a success, but an empty list
        // right after a click needs an explanation.
        setDiarizationReason(DIARIZATION_NO_SPEAKERS_FOUND_MESSAGE);
      }
    } finally {
      diarizingRef.current = false;
      setIsDiarizing(false);
    }
  };

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

  /**
   * Finds the rows a "this person is already assigned to…" error is about.
   *
   * Within one meeting a person holds only one label, so the conflict is a pair
   * of rows whose names are equal ignoring case and surrounding whitespace —
   * including a draft that just copies another row's saved name, which the
   * backend's message names by that row's old speaker number. Only rows whose
   * draft changed in this batch are reported, because they are what the user is
   * looking at; an empty result means the caller falls back to the message.
   */
  const conflictLabels = (): string[] => {
    const names = speakers.map((row) => ({
      label: row.label,
      name: foldName(draft[row.label] ?? null),
      changed: (draft[row.label] ?? null) !== (row.speaker_name ?? null),
    }));
    return names
      .filter(
        (row) =>
          row.changed &&
          row.name !== null &&
          names.some((other) => other.label !== row.label && other.name === row.name),
      )
      .map((row) => row.label);
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
      // Re-read the paginated transcripts so speaker chips show the new names
      // without a page reload.
      await onRefetchTranscripts?.();
    } catch (error) {
      const message = String(error);
      // The panel stays open so the user can fix the pair the backend rejected.
      const conflicting = conflictLabels();
      if (conflicting.length > 0) {
        setRowErrors(Object.fromEntries(conflicting.map((label) => [label, message])));
        return;
      }
      const conflict = /Speaker (\d+)/.exec(message);
      const label = conflict ? `speaker_${Number(conflict[1]) - 1}` : null;
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
      <SheetContent
        side="right"
        className="flex flex-col gap-4 sm:max-w-md"
        // Keep the transcript readable behind the sheet: the user matches names
        // against the lines. The overlay stays mounted, so an outside click
        // still dismisses the panel and drops the draft.
        overlayClassName="bg-transparent"
      >
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
            <div className="space-y-3 py-6 text-center">
              <p className="text-sm text-muted-foreground">{DIARIZATION_UNAVAILABLE_HINT}</p>
              <div className="flex justify-center">
                <Button
                  size="sm"
                  data-testid="diarize-meeting-button"
                  onClick={handleDiarize}
                  disabled={isDiarizing || !hasDiarizationModel}
                  title={
                    hasDiarizationModel
                      ? 'Run speaker diarization for this meeting'
                      : DIARIZATION_UNAVAILABLE_HINT
                  }
                >
                  {isDiarizing ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Sparkles className="mr-2 h-4 w-4" />
                  )}
                  {isDiarizing ? 'Running diarization…' : 'Run diarization'}
                </Button>
              </div>
              {diarizationReason && (
                <p
                  data-testid="diarization-reason"
                  className="text-sm text-red-600"
                >
                  {diarizationReason}
                </p>
              )}
            </div>
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
