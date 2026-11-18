# Speaker Names

Speaker diarization labels transcript segments with impersonal values (`speaker_0`, `speaker_1`, …). Speaker names let the user attach real people to those labels, for one meeting at a time. Vocabulary ("diarization", "speaker", "speaker name") is defined in [GLOSSARY.md](../GLOSSARY.md).

## Data model

Migration [20261118000000_add_speaker_names.sql](../frontend/src-tauri/migrations/20261118000000_add_speaker_names.sql) adds two tables:

- `speakers` — the global directory of people. `name` is globally unique ignoring case (`COLLATE NOCASE` index); names are immutable, so fixing a typo means unbinding the person everywhere and creating the name again.
- `meeting_speakers` — one row per named speaker of one meeting: `(meeting_id, label, speaker_id)`. `UNIQUE (meeting_id, label)` and `UNIQUE (meeting_id, speaker_id)` mean a label holds at most one person and a person holds at most one label per meeting. A row exists only while a name is set, so "No name" is a row deletion, and both foreign keys cascade.

`transcripts.speaker` stays the diarization label. A name is a separate entity resolved through `meeting_speakers` → `speakers`; this keeps every existing transcript contract (`Transcript`, `MeetingTranscript`, `transcripts.json`) intact. The additive `speaker_name` field on `MeetingTranscript` is what export and summary input read, so the frontend never joins.

The meeting's speaker list is not stored: it is `DISTINCT speaker` over the meeting's transcripts, left-joined to the assignments. Only `speaker_N` values count. Legacy `mic`/`system` values and free-form strings are audio sources, not speakers, and never appear in the panel or the dropdown.

## Commands

Registered in [lib.rs](../frontend/src-tauri/src/lib.rs); implementations live in [speakers](../frontend/src-tauri/src/database/repositories/speaker.rs) and [api.rs](../frontend/src-tauri/src/api/api.rs).

| Command | Payload | Returns |
| --- | --- | --- |
| `api_get_meeting_speakers` | `{ meetingId }` | `[{ label, speaker_name, segment_count, first_start_time, top_speaker_names }]` — one row per label, plus a bounded list of directory suggestions |
| `api_assign_meeting_speakers` | `{ meetingId, assignments: [{ label, name }] }` | The refreshed panel, same shape as above. `name: null` removes the name; the batch is one transaction, so either every assignment applies or none does |
| `api_set_transcript_speaker` | `{ transcriptId, label }` | `{ message }`. Sets one segment's label; `null` is "No speaker". The meeting's assignments are untouched |
| `api_get_speaker_names` | `{ query, limit }` | `[{ id, name, meeting_count }]` — directory search. `query` is a trimmed, case-insensitive substring; an exact match is listed first, then most-used, then alphabetical. `limit` defaults to 20 and is capped at 50 |

Names are trimmed and compared case-insensitively (including non-ASCII, which SQLite's `NOCASE` cannot fold, so comparison happens in Rust). Length is 1–100 characters after trimming; an empty name is an error, because removing a name is a separate action. Assigning two labels of one meeting to the same person fails with `this person is already assigned to Speaker N` and leaves the whole batch unapplied. Rebinding a label to another person is allowed — that is a choice, not a rename.

A person nobody references any more is deleted in the same transaction: when an assignment is removed, when a meeting is deleted, and when a recount drops the last binding that used the person. This keeps `api_get_speaker_names` and the panel suggestions free of orphans.

Successful assignments bump `meetings.updated_at`, like a title change; re-saving an unchanged panel is a no-op and does not.

## Recount rules

Labels are written in three places, all on a saved meeting: audio import ([import.rs](../frontend/src-tauri/src/audio/import.rs), `create_meeting_with_transcripts`), retranscription ([retranscription.rs](../frontend/src-tauri/src/audio/retranscription.rs), which deletes the transcripts first), and the shared diarization pass ([diarization_run.rs](../frontend/src-tauri/src/audio/transcription/diarization_run.rs), `persist_speaker_labels`). Each of them runs `reconcile_meeting_speaker_assignments` inside the same transaction that writes the labels:

- Labels that still exist keep their names, even when their segment counts changed.
- Bindings of labels that disappeared are deleted, and so are the people left unreferenced.
- If the label set changed such that none of the old labels survived, every binding of the meeting is gone and the backend emits [`speaker-names-reset`](#speaker-names-reset). Names are never guessed by order of appearance — the project has no voice embeddings.

Import and retranscription no longer diarize inline: they commit the meeting with unlabelled segments (`speaker = null`) and then schedule the saved-meeting diarization pass, which re-decodes the meeting's audio file and writes the labels through `persist_speaker_labels`. In retranscription the first recount is real — the rewritten transcript can lose labels whose names must go with them. In import it is a formality: a brand-new meeting has no bindings to lose, but it runs anyway so all three paths stay uniform. The pass's recount then aligns the names with the labels it just produced, in the same transaction as the label write.

## `speaker-names-reset`

Emitted (payload `{ meeting_id, message }`) by the recount paths above when names were lost. `message` is the user-facing explanation ("Speaker names were reset because the voices were recomputed."). [useSpeakerNamesResetListener.ts](../frontend/src/hooks/useSpeakerNamesResetListener.ts) shows it as a toast and refetches the open meeting's transcripts and speaker list. The event always names a meeting: an import saves the meeting before the diarization pass runs, so by the time a recount can drop names there is an id to report, and the page only reacts to its own meeting.

## Where names appear

- **Transcript chip** — [VirtualizedTranscriptView.tsx](../frontend/src/components/VirtualizedTranscriptView.tsx) shows the assigned name, else `Speaker N` (the number comes from the label suffix, not from page order). The chip is a dropdown listing the meeting's speakers plus "No speaker"; it is available only on the meeting page, not during live recording.
- **Speakers panel** — [SpeakersPanel.tsx](../frontend/src/components/MeetingDetails/SpeakersPanel.tsx), a right-hand sheet over a transparent overlay so the transcript stays readable. Every edit stays in a local draft; **Done** sends one batch assignment, while **Cancel**, Escape and an outside click discard the draft.
- **Copy and export** — [transcript-format.ts](../frontend/src/lib/transcript-format.ts) renders `[00:00–00:24] Alex: …`; an unnamed label still renders as `Speaker N`, and a segment without a label gets no prefix. Raw `speaker_N` never reaches the file. When no segment has a diarization label, the legacy `[MM:SS] text` format is kept byte-for-byte.
- **Summary input** — [summary-transcript-payload.ts](../frontend/src/lib/summary-transcript-payload.ts) renders `[MM:SS] Alex: text`. The summary cache invalidates itself because its fingerprint is computed from the final text, so existing summaries are not regenerated automatically.
