-- Migration: transcribe.cpp compute mode («режим вычислений»)
-- `transcribeBackend` stores the Transcribe.cpp compute-mode selection
-- ("auto" | "vulkan" | "cpu") next to the provider/model pair so the choice
-- survives restarts. NOT NULL DEFAULT 'auto': existing rows keep their
-- provider and model and read back as Auto, the behaviour they had while no
-- selector existed. Only the transcribeCpp provider reads this column.

ALTER TABLE transcript_settings ADD COLUMN transcribeBackend TEXT NOT NULL DEFAULT 'auto';
