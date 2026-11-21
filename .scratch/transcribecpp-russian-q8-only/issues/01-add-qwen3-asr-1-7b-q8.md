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

**Status:** resolved

- [x] A `Qwen3-ASR-1.7B-Q8_0` catalog entry exists with display name
      "Qwen3-ASR 1.7B", lowercase family `qwen3-asr-1.7b`, and a description
      in the style of the Qwen3-ASR 0.6B entry (multilingual, strongest
      Russian accuracy family).
- [x] Its source URL pins a commit in the handy-computer 1.7B GGUF
      repository, and its exact byte size and lowercase-hex SHA-256 were
      verified against the Hugging Face LFS metadata of that commit (upstream
      catalog reports 2 185 030 624 bytes; re-verify, do not copy blindly).
- [x] Model id equals the filename stem; `size_mb` is the exact byte count
      rounded to MiB; the catalog-wide invariants tests pass unchanged.
- [ ] The entry appears in the settings model list in the Tauri app (needs manual check; catalog invariant tests cover metadata correctness), next to
      Qwen3-ASR 0.6B, and downloads validate end to end (or, if the full
      download is impractical to demo, discovery reports Missing with the
      pinned size and a hash-valid manual artifact reports Available).
- [x] Catalog count assertions in the test suite updated for the widened
      temporary state (this ticket's state is expected to be superseded by
      ticket 02's final ten-entry assertions).

## Comments

- 2026-11-21: Implemented on branch `ticket/01-add-qwen3-asr-1.7b` (commit `656fc2a`), merged into `feature/transcribecpp-russian-q8-only` (`693a378`). Metadata re-verified against the HF API: commit `0e0a596e3959c3f3f2ab7d443a5b6f0f4a4bba6f`, 2 185 030 624 bytes (2084 MiB), sha256 `9a0d81792dfea2d5f278b8a63deb3ea6e02139ce42c2301f32ea19c4f77526b7`. Tauri-app UI verification remains a manual check.
