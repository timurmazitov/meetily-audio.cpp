-- Migration: Add editable base system prompt for summary generation
-- Stores the user-configurable base system prompt; NULL means "use the built-in default".

ALTER TABLE settings ADD COLUMN summarySystemPrompt TEXT;
