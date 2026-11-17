#!/usr/bin/env python3
"""Validate the packages attached to a release, and build the updater manifest.

Two modes, chosen by the caller: the script owns no knowledge of the bundler
config it is validating.

* No public key given:
  every expected package type must be present exactly once. Signatures are not
  checked, ``latest.json`` is not created, and the skipped manifest is stated
  explicitly.
* Public key given (updater enabled): signatures are additionally
  required for every signed package type, verified with ``minisign``, and
  ``latest.json`` is written in the format the Tauri updater expects.

macOS packages are intentionally absent: the release pipeline does not build
macOS (no Apple Developer certificate in CI). Re-add the dmg/app-tarball
entries to both ``PACKAGES`` and ``PLATFORM_ORDER`` when macOS builds return.

Usage:

    python3 scripts/check-release-assets.py ASSETS_DIR VERSION OWNER/REPO
    python3 scripts/check-release-assets.py ASSETS_DIR VERSION OWNER/REPO \\
        --public-key PATH/TO/public.key

``ASSETS_DIR`` holds already-downloaded release assets; no network access is
performed. Exit status is 0 on success and non-zero on any problem, with a
human-readable message naming the offending package type and the files actually
found.
"""
import argparse
import base64
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Iterable, Sequence
from urllib.parse import quote


class MinisignError(Exception):
    """Raised when a package signature cannot be verified."""


@dataclass(frozen=True)
class Package:
    """One expected release package type.

    ``glob`` finds the asset, ``signed`` says whether an updater ``.sig`` file
    must accompany it, and ``updater_artifact`` marks files that only exist when
    the bundler emits updater artifacts (``createUpdaterArtifacts``).
    """

    name: str
    glob: str
    signed: bool
    updater_artifact: bool = False


# Single source of truth: type -> glob -> signed. Editing the bundle config
# (adding a target, turning updater artifacts on or off) is a one-line edit
# here.
PACKAGES: tuple = (
    Package("msi", "*64_en-US.msi", signed=True),
    Package("nsis installer", "*-setup.exe", signed=True),
    Package("deb", "*.deb", signed=True),
    Package("appimage", "*.AppImage", signed=True),
)

MANIFEST_NAME = "latest.json"

# Manifest key order, kept identical to the format the inline workflow heredoc
# produced. Each target names the package type that feeds it. macOS targets are
# omitted: the release pipeline does not build macOS (see the module docstring).
PLATFORM_ORDER = (
    ("windows-x86_64-nsis", "nsis installer"),
    ("windows-x86_64-msi", "msi"),
    ("windows-x86_64", "nsis installer"),
    ("linux-x86_64-deb", "deb"),
    ("linux-x86_64-appimage", "appimage"),
)


def find_packages(assets_dir: Path, packages: Iterable = PACKAGES):
    """Resolve every package type to its single matching file.

    Returns ``(matches, errors)`` where ``matches`` maps package name to path.
    """
    matches = {}
    errors = []
    for package in packages:
        found = sorted(p for p in assets_dir.glob(package.glob) if p.is_file())
        if len(found) != 1:
            names = ", ".join(p.name for p in found) or "none"
            errors.append(
                f"expected exactly one {package.name} package matching "
                f"'{package.glob}', found {len(found)}: [{names}]"
            )
            continue
        matches[package.name] = found[0]
    return matches, errors


def verify_with_minisign(asset: Path, public_key: Path, signature: Path) -> None:
    """Verify ``asset`` against ``public_key`` using the ``minisign`` binary.

    ``.sig`` files hold a base64-encoded minisign signature, so the payload is
    decoded into a temporary file first. Raises :class:`MinisignError` on any
    failure and never logs key or signature contents.
    """
    try:
        decoded = base64.b64decode(signature.read_text().strip(), validate=True)
    except (OSError, ValueError) as exc:
        raise MinisignError(f"unreadable signature {signature.name}: {exc}") from exc
    with tempfile.TemporaryDirectory() as work_dir:
        decoded_path = Path(work_dir) / "signature.minisig"
        decoded_path.write_bytes(decoded)
        completed = subprocess.run(
            ["minisign", "-Vm", str(asset), "-p", str(public_key), "-x", str(decoded_path)],
            capture_output=True,
            text=True,
        )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).strip().splitlines()
        raise MinisignError(detail[-1] if detail else "minisign failed")


def write_manifest(assets_dir: Path, version: str, repo: str, matches) -> Path:
    """Write ``latest.json`` for an enabled updater and return its path."""
    platforms = {}
    for target, package_name in PLATFORM_ORDER:
        asset = matches[package_name]
        platforms[target] = {
            "url": f"https://github.com/{repo}/releases/download/v{version}/{quote(asset.name)}",
            "signature": Path(str(asset) + ".sig").read_text().strip(),
        }
    manifest = {
        "version": version,
        "notes": "",
        "pub_date": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "platforms": platforms,
    }
    path = assets_dir / MANIFEST_NAME
    path.write_text(json.dumps(manifest, indent=2) + "\n")
    return path


def parse_args(argv: Sequence[str]):
    parser = argparse.ArgumentParser(
        description="Validate downloaded release packages; build latest.json when a public key is given.",
    )
    parser.add_argument("assets_dir", type=Path, help="directory with downloaded release assets")
    parser.add_argument("version", help="release version without the leading 'v'")
    parser.add_argument("repo", help="GitHub repository as owner/name")
    parser.add_argument(
        "--public-key",
        type=Path,
        help="path to the updater public key; enables signature checks and latest.json",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str], run_minisign: Callable = None) -> int:
    """Run the validation. Returns the process exit code."""
    args = parse_args(argv)
    verify = run_minisign or verify_with_minisign

    errors = []
    if not args.assets_dir.is_dir():
        _report([f"assets directory not found: {args.assets_dir}"])
        return 1
    if "/" not in args.repo or args.repo.count("/") != 1:
        errors.append(f"repository must be owner/name, got '{args.repo}'")

    updater_enabled = args.public_key is not None
    if updater_enabled and not args.public_key.is_file():
        errors.append(f"public key file not found: {args.public_key}")

    required = [p for p in PACKAGES if updater_enabled or not p.updater_artifact]
    matches, find_errors = find_packages(args.assets_dir, required)
    errors.extend(find_errors)
    if errors:
        _report(errors)
        return 1

    for package in required:
        if not updater_enabled or not package.signed:
            continue
        asset = matches[package.name]
        signature = Path(str(asset) + ".sig")
        if not signature.is_file():
            errors.append(f"missing signature for {package.name} package: expected {signature.name}")
            continue
        try:
            verify(asset, args.public_key, signature)
        except Exception as exc:  # a swapped-in verifier may raise anything
            errors.append(f"signature verification failed for {asset.name}: {exc}")
    if errors:
        _report(errors)
        return 1

    for package in required:
        print(f"ok: {package.name}: {matches[package.name].name}")

    if updater_enabled:
        manifest = write_manifest(args.assets_dir, args.version, args.repo, matches)
        print(f"ok: wrote {manifest.name} for version {args.version}")
    else:
        print(
            "no public key given: signatures not checked, "
            f"{MANIFEST_NAME} not created"
        )
    return 0


def _report(errors: Sequence[str]) -> None:
    print("release asset validation failed:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
