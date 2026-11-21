# 02: Shrink the transcribe.cpp catalog to the ten Russian Q8_0 models

**What to build:** The settings transcription section shows exactly ten
models — all Q8_0, all Russian-capable per the upstream transcribe.cpp
README and its catalog metadata: GigaAM v3 CTC, e2e CTC, RNN-T, e2e RNN-T;
Parakeet TDT 0.6B v3; Parakeet Ultra; Qwen3-ASR 0.6B; Qwen3-ASR 1.7B;
Whisper large-v3 Turbo; Whisper large-v3. Every F16, Q6_K, Q5_K_M and Q4_K_M
entry is gone, so the quantization choice disappears from the user's view.
Display order stays Russian-first, then multilingual, then Whisper. Already
downloaded artifacts on disk are left untouched; no settings or database
migration.

**Blocked by:** 01 (Add Qwen3-ASR 1.7B (Q8_0) to the transcribe.cpp catalog) —
the final ten-entry list includes that model, and the ten-entry test
assertions must land against a complete catalog.

**Status:** ready-for-agent

- [ ] The catalog contains exactly the ten Q8_0 entries listed above and
      nothing else; the catalog tests assert this exact membership, the
      all-Q8_0 invariant, and a hardcoded per-family Russian-support
      allow-list sourced from the upstream README.
- [ ] The catalog module docs (header and any comments describing the old
      five-quantization policy) are rewritten to state the Russian + Q8_0
      policy and name the upstream README as its source, so future edits
      respect the filter.
- [ ] The default-model and onboarding tests keep passing
      (`parakeet-ultra-Q8_0` remains catalogued and default).
- [ ] Tests that referenced removed entries (e.g. digest/corruption cases
      built from a removed quantization) are re-pointed at surviving entries.
- [ ] Dispatcher-side tests that resolve catalogued models keep passing;
      no frontend changes are made (the manager renders whatever the command
      returns; the client-side quantization union may stay wide).
- [ ] Verified in the Tauri app: the settings transcription section lists
      exactly the ten models with their sizes; an installed-but-removed model
      (if any) no longer appears and its file remains on disk.
- [ ] Rust catalog/models tests for the transcribe.cpp engine pass on Linux.
