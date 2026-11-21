# 01: Add Qwen3-ASR 1.7B (Q8_0) to the transcribe.cpp catalog

**What to build:** The settings transcription section gains one new model,
"Qwen3-ASR 1.7B", downloadable and selectable like the existing entries. Its
artifact metadata is verified against the Hugging Face API before merging —
the pinned repository commit, the exact artifact size and the SHA-256 digest
follow the same recipe the catalog module docs already prescribe (the
`api/models/<repo>` endpoint for the commit, `tree/main` for the LFS
`oid`/`size`). The catalog is temporarily wider (46 entries); all existing
tests stay green.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

- [ ] A `Qwen3-ASR-1.7B-Q8_0` catalog entry exists with display name
      "Qwen3-ASR 1.7B", lowercase family `qwen3-asr-1.7b`, and a description
      in the style of the Qwen3-ASR 0.6B entry (multilingual, strongest
      Russian accuracy family).
- [ ] Its source URL pins a commit in the handy-computer 1.7B GGUF
      repository, and its exact byte size and lowercase-hex SHA-256 were
      verified against the Hugging Face LFS metadata of that commit (upstream
      catalog reports 2 185 030 624 bytes; re-verify, do not copy blindly).
- [ ] Model id equals the filename stem; `size_mb` is the exact byte count
      rounded to MiB; the catalog-wide invariants tests pass unchanged.
- [ ] The entry appears in the settings model list in the Tauri app, next to
      Qwen3-ASR 0.6B, and downloads validate end to end (or, if the full
      download is impractical to demo, discovery reports Missing with the
      pinned size and a hash-valid manual artifact reports Available).
- [ ] Catalog count assertions in the test suite updated for the widened
      temporary state (this ticket's state is expected to be superseded by
      ticket 02's final ten-entry assertions).
