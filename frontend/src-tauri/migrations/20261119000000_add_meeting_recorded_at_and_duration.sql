-- Migration: add the meeting's own moment and length
-- `recorded_at` is when the meeting happened (start of the recording, or the
-- moment of import); it is deliberately separate from `created_at`, which stays
-- the audit field "when the row appeared". `duration_seconds` is how long it
-- lasted. Both are nullable and have no DEFAULT: NULL means "unknown" and is
-- never written as zero or as the current moment.

ALTER TABLE meetings ADD COLUMN recorded_at TEXT;
ALTER TABLE meetings ADD COLUMN duration_seconds REAL;
