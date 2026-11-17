#!/usr/bin/env python3
"""Brand lint: fail if the old brand string appears outside explicitly allowed zones.

Run from the repo root: python3 scripts/check-brand.py
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Version-control metadata is never repository content and is skipped wholesale.
# `.git` is a directory in a normal clone, but a *file* in a linked worktree
# (`gitdir: /path/to/.git/worktrees/<name>`); the path inside it may itself contain
# the old brand, so it must be skipped as a file too, not only pruned as a directory.
VCS_METADATA = {".git", ".hg", ".svn", ".bzr", "CVS"}

EXCLUDED_DIRS = {
    "node_modules", "target", ".next", "dist", ".turbo",
    ".scratch",       # planning docs; historical references are facts
}
# Repo-root-relative paths, compared as normalized `a/b/c` paths on every platform.
# An entry without a separator is just a root-level path: excluding `LICENSE.md`
# does NOT exclude a same-named file in a subdirectory, so an exclusion never
# silently spreads to unrelated files.
EXCLUDED_FILES = {
    "LICENSE.md",     # attribution: MIT copyright of the original project is preserved here by design
    "docs/research/upstream-0.5.0.md",  # research *about* the upstream project: its name and paths are the subject,
                                        # not a brand regression; other docs under docs/research/ stay checked
}

# Line-level allowlist: lines matching any of these may mention the old brand.
ALLOWED_LINE_PATTERNS = [
    re.compile(r"/opt/homebrew/var/meetily/", re.I),          # legacy Meetily Homebrew DB path (import source)
    re.compile(r"/usr/local/var/meetily/", re.I),             # legacy Meetily Homebrew DB path (import source)
    re.compile(r"Previous Meetily Installation", re.I),       # UI copy about detecting a legacy Meetily install
    re.compile(r"previous Meetily installation", re.I),       #   (the old product's name is the fact being described)
    re.compile(r"Meetily -> Samobranka", re.I),               # TODO comments describing the rebrand itself
    re.compile(r"based on Meetily", re.I),                    # allowed factual mentions
    re.compile(r"BRAND = re.compile"),                        # this script's own pattern definition
    re.compile(r"из Meetily", re.I),                          # ADR: factual "renamed from Meetily"
]

# Files that may contain the old brand anywhere (compiled artifacts, to be regenerated on next build).
ALLOWED_FILES = [
    re.compile(r"frontend/src-tauri/binaries/"),              # prebuilt sidecars with embedded old-brand strings
]

BRAND = re.compile(r"meetily", re.I)

failures = []
for dirpath, dirnames, filenames in os.walk(ROOT):
    dirnames[:] = [d for d in dirnames if d not in EXCLUDED_DIRS and d not in VCS_METADATA]
    for name in filenames:
        rel = os.path.relpath(os.path.join(dirpath, name), ROOT).replace(os.sep, "/")
        if name in VCS_METADATA or rel in EXCLUDED_FILES:
            continue
        if any(p.search(rel) for p in ALLOWED_FILES):
            continue
        if BRAND.search(name):
            failures.append(f"FILENAME: {rel}")
        try:
            with open(os.path.join(dirpath, name), encoding="utf-8", errors="strict") as f:
                for lineno, line in enumerate(f, 1):
                    if BRAND.search(line) and not any(p.search(line) for p in ALLOWED_LINE_PATTERNS):
                        failures.append(f"{rel}:{lineno}: {line.strip()[:120]}")
        except (UnicodeDecodeError, OSError):
            continue  # binary/unreadable file: content not inspected, name still checked above

if failures:
    print(f"FAIL: {len(failures)} old-brand occurrence(s) outside allowed zones:\n")
    print("\n".join(failures))
    sys.exit(1)
print("OK: no old-brand occurrences outside allowed zones")
