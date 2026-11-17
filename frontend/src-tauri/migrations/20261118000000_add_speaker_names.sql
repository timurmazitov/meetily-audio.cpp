-- Add a global directory of people and the per-meeting speaker assignments.
-- `transcripts.speaker` stays the diarization label (`speaker_N`); a real name
-- is a separate entity resolved through `meeting_speakers` → `speakers`.
CREATE TABLE speakers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_speakers_name ON speakers(name COLLATE NOCASE);

-- A row exists only while a name is assigned, so removing a name = deleting
-- the row; `speaker_id NOT NULL` and the cascade leave no leftovers.
CREATE TABLE meeting_speakers (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    label TEXT NOT NULL,
    speaker_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (speaker_id) REFERENCES speakers(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX idx_meeting_speakers_label ON meeting_speakers(meeting_id, label);
CREATE UNIQUE INDEX idx_meeting_speakers_person ON meeting_speakers(meeting_id, speaker_id);
