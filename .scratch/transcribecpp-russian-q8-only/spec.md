# Spec: Russian-only Q8_0 transcribe.cpp model list in settings

Status: ready-for-agent

## Problem Statement

The transcription settings currently offer the user 45 transcribe.cpp models:
nine variants across five quantizations (F16, Q8_0, Q6_K, Q5_K_M, Q4_K_M).
For this Russian-language product most of that list is noise: the user cannot
judge which models handle Russian, and the quantization choices add decisions
the user should not have to make (the app has already concluded Q8_0 is the
right accuracy/size trade-off). Users see a long, confusing list where the
relevant options are buried.

## Solution

Reduce the transcribe.cpp model catalog to exactly the models that (a) support
Russian and (b) are published in the Q8_0 quantization, sourced from the
upstream transcribe.cpp model list
(https://github.com/handy-computer/transcribe.cpp/blob/main/README.md and its
`catalog/` metadata). The settings transcription section then shows a short,
curated list of ten Russian-capable Q8_0 models; nothing else changes in the
settings UI, download flow, or transcription pipeline.

The catalog is the single source of truth for the settings list, so shrinking
it is the whole change: the model manager, the selection dropdown, the
download manager, the readiness gates and the dispatcher all read the same
catalog.

## User Stories

1. As a Russian-speaking user, I want the transcription settings to show only models that support Russian, so that every model I can pick actually works for my meetings.
2. As a user, I want only Q8_0 quantizations listed, so that I do not have to understand quantization trade-offs to choose a model.
3. As a user, I want the list to be short (ten entries), so that I can compare the options at a glance instead of scrolling through 45.
4. As a user, I want the curated list to match the upstream transcribe.cpp model list, so that the app offers what the engine officially supports and verifies.
5. As a user, I want to keep the GigaAM variants in the list, so that I still have the strongest dedicated Russian models available.
6. As a user, I want the multilingual options (Parakeet Ultra / TDT v3, Qwen3-ASR 0.6B / 1.7B, Whisper large-v3 / large-v3 Turbo) to remain, so that mixed Russian/English meetings are still covered.
7. As a new user, I want the default model I get out of the box to appear in the list, so that the app is coherent from first launch (the default `parakeet-ultra-Q8_0` is Russian-capable at Q8_0).
8. As a user with an already-installed model, I want that model to keep working after the update, so that the change does not break my existing setup (every previously offered model is either Russian-capable at Q8_0 or remains selectable while installed).
9. As a user, I want download size and checksum validation to remain exact for every listed model, so that downloads stay trustworthy after the catalog shrinks.
10. As a user, I want the onboarding flow to still select a valid default model, so that first-run setup keeps working unchanged.
11. As a developer, I want the catalog tests to pin the exact ten-entry list, so that an accidental re-addition of a non-Russian or non-Q8_0 model fails CI.
12. As a developer, I want the catalog to record why it is filtered (Russian + Q8_0 policy), so that a future maintainer does not "restore" the removed entries.

## Implementation Decisions

- **Single seam, no new seams.** The filter lives in the Rust static catalog
  (`transcribe_cpp_engine` catalog module) itself. No new command, no new
  frontend filter, no runtime language/quant filter layer. The highest existing
  seam — `catalog()` / `find_model()` — is the only surface.
- **Final catalog contents** (10 entries, one Q8_0 variant each), all taken
  from the upstream README table whose `catalog/*.json` lists `ru` in
  `languages`:
  1. `gigaam-v3-ctc-Q8_0`
  2. `gigaam-v3-e2e-ctc-Q8_0`
  3. `gigaam-v3-rnnt-Q8_0`
  4. `gigaam-v3-e2e-rnnt-Q8_0`
  5. `parakeet-tdt-0.6b-v3-Q8_0`
  6. `parakeet-ultra-Q8_0` (remains the app default)
  7. `qwen3-asr-0.6b` package `Qwen3-ASR-0.6B-Q8_0`
  8. `Qwen3-ASR-1.7B-Q8_0` — new entry, from the `handy-computer/Qwen3-ASR-1.7B-gguf`
     repository; upstream publishes it at 2 185 030 624 bytes (~2084 MiB).
     Display name "Qwen3-ASR 1.7B"; description follows the 0.6B wording.
  9. `whisper-large-v3-turbo-Q8_0`
  10. `whisper-large-v3-Q8_0`
- **Removed from the catalog**: every F16/Q6_K/Q5_K_M/Q4_K_M entry (35 rows)
  and the `model_catalog!` macro's multi-quant form is simplified to one
  quantization per variant (the macro can keep its shape or be flattened —
  implementer's choice, no behavioral requirement).
- **New entry metadata must be pinned the same way as the existing ones**:
  exact byte size and lowercase-hex SHA-256 verified against the Hugging Face
  API (`api/models/<repo>` for the pinned commit, `tree/main` for the LFS
  `oid`/`size`) before merging, and the source URL pinned to a commit, per the
  catalog module's existing doc contract.
- **Display order is preserved**: Russian-first (GigaAM), then multilingual,
  then Whisper, with the 1.7B Qwen placed next to the 0.6B one.
- **No frontend changes required.** The TypeScript quantization union in the
  transcribe.cpp client library may stay wide (it mirrors the serde shape, not
  the current membership); the manager renders whatever the command returns.
- **No data migration.** Models already downloaded but no longer listed become
  invisible to the manager; the on-disk artifacts are left alone. Existing
  settings rows that reference a still-listed model (including the default
  `parakeet-ultra-Q8_0`) keep working. The dispatcher and onboarding resolve
  models through `find_model`, so they automatically agree with the new list.
- **Tests in the catalog module are the behavioral contract**: the test suite
  is updated in place to assert the ten-entry membership (per the Testing
  Decisions), replacing the current "9 variants × 5 quants = 45" assertions.

## Testing Decisions

- Good tests assert external behavior only: the catalog's membership, the
  pinned-metadata invariants, and the serialization shape the UI consumes —
  never internal helper structure.
- The existing Rust test module in the catalog file is the test seam (prior
  art: `catalog.rs` tests, `models.rs` download-lifecycle tests, and the
  dispatcher tests in `transcribe_cpp_engine/transcription.rs` which build
  `ModelInfo` via `find_model`). No new test files or seams are needed.
- Updated/added assertions:
  - The catalog contains exactly ten entries, all with quantization `Q8_0`,
    and the expected family set (four GigaAM, two Parakeet, two Qwen, two
    Whisper).
  - Every entry's family is one whose upstream `catalog/<variant>.json` lists
    `ru` — asserted as a hardcoded allow-list in the test, mirroring how the
    upstream README is treated as the source of truth.
  - `size_mb` rounding, unique names, filename-stem id rule, commit-pinned
    handy-computer source URLs, and lowercase-hex SHA-256 checks continue to
    run over the ten entries unchanged.
  - The default-model test keeps passing (`parakeet-ultra-Q8_0` still
    catalogued); onboarding tests keep passing unchanged.
  - The `Corrupted`/digest tests referencing `whisper-large-v3-Q4_K_M` must be
    re-pointed at a surviving entry (e.g. `whisper-large-v3-Q8_0`).
- Frontend tests (Bun component tests for the model manager) need no changes
  because the manager takes its list from the injected client.

## Out of Scope

- Adding other Russian-capable upstream variants (canary-1b-v2,
  nemotron-3.5-asr-streaming-0.6b, orukeet, parakeet-primeline,
  voxtral-mini-4b-realtime-2602, small Whisper sizes) — the user explicitly
  chose only the current nine plus the large Qwen 1.7B.
- Any change to diarization (audio.cpp) model management.
- Deleting on-disk artifacts of models removed from the catalog, or any
  settings/database migration for stale model references.
- Language selection UI, per-meeting language choice, or runtime language
  detection.
- Changing the default model, compute modes, download mechanism, or the
  transcription pipeline itself.
- Any change to cloud summary/transcription providers.

## Further Notes

- Russian support was verified against the upstream `catalog/*.json`
  `languages` field of transcribe.cpp main (2026-11-21): the current nine
  catalogued variants all list `ru`; `Qwen3-ASR-1.7B` lists `ru` among 30
  languages.
- The catalog module's header doc (which currently explains the five-offered-
  quant policy) must be rewritten to state the new Russian + Q8_0 policy and
  its source (the upstream README), so future edits respect the filter.
- If upstream later re-quantizes or re-publishes any pinned artifact, the
  size/digest re-verification recipe in the module doc applies unchanged.
