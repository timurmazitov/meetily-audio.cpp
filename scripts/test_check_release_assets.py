#!/usr/bin/env python3
"""Tests for scripts/check-release-assets.py.

Run from the repo root:

    python3 -m unittest discover -s scripts

or directly:

    python3 scripts/test_check_release_assets.py

Standard library only (unittest). The file name uses underscores because
``unittest discover`` only imports module-name-safe files; the script it tests
keeps the repository's hyphenated convention. Fixtures are empty files with
realistic names inside a temporary directory, so the tests never write into the
repository and never need a real minisign key: the minisign call is injected via
``main(..., run_minisign=<callable>)``.
"""
import base64
import contextlib
import importlib.util
import io
import json
import os
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

SCRIPT_PATH = Path(__file__).with_name("check-release-assets.py")

VERSION = "0.5.0"
REPO = "timurmazitov/samobranka"

# Realistic asset names.
PACKAGE_FILES = {
    "msi": "samobranka_0.5.0_x64_en-US.msi",
    "nsis": "samobranka_0.5.0_x64-setup.exe",
    "deb": "samobranka_0.5.0_amd64.deb",
    "appimage": "samobranka_0.5.0_amd64.AppImage",
}
SIGNED_KINDS = ("msi", "nsis", "deb", "appimage")
SIGNATURE_BODY = base64.b64encode(b"not-a-real-minisign-signature").decode()


def load_script():
    spec = importlib.util.spec_from_file_location("check_release_assets", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReleaseAssetsTest(unittest.TestCase):
    def setUp(self):
        self.module = load_script()
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.root = Path(self._tmp.name)
        self.assets = self.root / "release-assets"
        self.assets.mkdir()
        self.key = self.root / "public.key"

    def write_assets(self, *, omit=(), extra=(), signatures=False):
        for kind, name in PACKAGE_FILES.items():
            if kind in omit:
                continue
            (self.assets / name).write_bytes(b"")
            if signatures and kind in SIGNED_KINDS:
                (self.assets / (name + ".sig")).write_text(SIGNATURE_BODY)
        for name in extra:
            (self.assets / name).write_bytes(b"")

    def write_key(self):
        self.key.write_text("untrusted comment: test key\n")
        return self.key

    def run_script(self, *args, run_minisign=None):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = self.module.main(list(args), run_minisign=run_minisign)
        return code, stdout.getvalue(), stderr.getvalue()

    # 1. Full package set, no public key -> success, manifest not created.
    def test_full_set_without_key_succeeds_and_skips_manifest(self):
        self.write_assets()
        code, out, err = self.run_script(str(self.assets), VERSION, REPO)
        self.assertEqual(code, 0, err)
        self.assertFalse((self.assets / "latest.json").exists())
        self.assertIn("no public key given", out)
        self.assertIn("latest.json", out)

    # 2. One package type missing -> error naming the missing type.
    def test_missing_package_type_is_reported(self):
        self.write_assets(omit=("msi",))
        code, out, err = self.run_script(str(self.assets), VERSION, REPO)
        self.assertNotEqual(code, 0)
        self.assertIn("msi", err)
        self.assertIn("found 0", err)
        self.assertIn("64_en-US.msi", err)

    # 3. Two packages of one type -> error naming the duplicate.
    def test_duplicate_package_type_is_reported(self):
        self.write_assets(extra=("samobranka_0.5.0_arm64.deb",))
        code, out, err = self.run_script(str(self.assets), VERSION, REPO)
        self.assertNotEqual(code, 0)
        self.assertIn("found 2", err)
        self.assertIn("samobranka_0.5.0_amd64.deb", err)
        self.assertIn("samobranka_0.5.0_arm64.deb", err)

    # 4. Public key present, every signed package has a .sig -> success and a
    #    complete manifest.
    def test_signed_packages_with_key_produce_manifest(self):
        self.write_assets(signatures=True)
        key = self.write_key()
        verified = []

        def fake_minisign(asset, public_key, signature):
            verified.append(asset.name)

        code, out, err = self.run_script(
            str(self.assets), VERSION, REPO, "--public-key", str(key),
            run_minisign=fake_minisign,
        )
        self.assertEqual(code, 0, err)
        manifest_path = self.assets / "latest.json"
        self.assertTrue(manifest_path.exists())
        manifest = json.loads(manifest_path.read_text())

        self.assertEqual(manifest["version"], VERSION)
        self.assertEqual(manifest["notes"], "")
        self.assertEqual(
            sorted(manifest["platforms"]),
            sorted([
                "windows-x86_64-nsis", "windows-x86_64-msi", "windows-x86_64",
                "linux-x86_64-deb", "linux-x86_64-appimage",
            ]),
        )
        self.assertEqual(len(verified), len(SIGNED_KINDS), verified)
        for entry in manifest["platforms"].values():
            self.assertEqual(entry["signature"], SIGNATURE_BODY)
            self.assertIn(f"v{VERSION}/", entry["url"])

        # Platform keys appear in the same order as the historical manifest.
        self.assertEqual(
            list(manifest["platforms"]),
            [
                "windows-x86_64-nsis", "windows-x86_64-msi", "windows-x86_64",
                "linux-x86_64-deb", "linux-x86_64-appimage",
            ],
        )

        # Trailing newline and 2-space indentation match the historical format.
        raw = manifest_path.read_text()
        self.assertTrue(raw.endswith("\n"))
        self.assertIn('\n  "version"', raw)

    # Additional: pub_date is parseable ISO-8601 UTC.
    def test_manifest_pub_date_is_iso8601_utc(self):
        self.write_assets(signatures=True)
        key = self.write_key()
        code, _, err = self.run_script(
            str(self.assets), VERSION, REPO, "--public-key", str(key),
            run_minisign=lambda *a: None,
        )
        self.assertEqual(code, 0, err)
        pub_date = json.loads((self.assets / "latest.json").read_text())["pub_date"]
        self.assertTrue(pub_date.endswith("Z"), pub_date)
        parsed = datetime.fromisoformat(pub_date.replace("Z", "+00:00"))
        self.assertEqual(parsed.utcoffset(), timedelta(0))
        self.assertLess(abs((datetime.now(timezone.utc) - parsed).total_seconds()), 300)

    # Additional: the URL carries the version and never contains raw spaces.
    def test_manifest_url_encodes_file_name(self):
        self.write_assets(signatures=True)
        key = self.write_key()
        code, _, err = self.run_script(
            str(self.assets), VERSION, REPO, "--public-key", str(key),
            run_minisign=lambda *a: None,
        )
        self.assertEqual(code, 0, err)
        platforms = json.loads((self.assets / "latest.json").read_text())["platforms"]
        expected_url = (
            f"https://github.com/{REPO}/releases/download/v{VERSION}/"
            "samobranka_0.5.0_amd64.AppImage"
        )
        self.assertEqual(platforms["linux-x86_64-appimage"]["url"], expected_url)
        for entry in platforms.values():
            self.assertIn(f"v{VERSION}/", entry["url"])
            self.assertNotIn(" ", entry["url"])

    # 5. Public key present, one signed package has no .sig -> error.
    def test_missing_signature_is_reported(self):
        self.write_assets(signatures=True)
        (self.assets / (PACKAGE_FILES["deb"] + ".sig")).unlink()
        key = self.write_key()
        code, _, err = self.run_script(
            str(self.assets), VERSION, REPO, "--public-key", str(key),
            run_minisign=lambda *a: None,
        )
        self.assertNotEqual(code, 0)
        self.assertIn(PACKAGE_FILES["deb"] + ".sig", err)
        self.assertFalse((self.assets / "latest.json").exists())

    # 6. Signature verification fails (swapped-in minisign) -> error.
    def test_signature_verification_failure_is_reported(self):
        self.write_assets(signatures=True)
        key = self.write_key()

        def failing_minisign(asset, public_key, signature):
            raise self.module.MinisignError("signature is invalid")

        code, _, err = self.run_script(
            str(self.assets), VERSION, REPO, "--public-key", str(key),
            run_minisign=failing_minisign,
        )
        self.assertNotEqual(code, 0)
        self.assertIn("verification failed", err)
        self.assertFalse((self.assets / "latest.json").exists())

    # Additional: no-key mode must still not require .sig files.
    def test_without_key_signatures_are_not_required(self):
        self.write_assets(signatures=False)
        code, _, err = self.run_script(str(self.assets), VERSION, REPO)
        self.assertEqual(code, 0, err)

    # Additional: the real minisign adapter decodes the base64 .sig and shells
    # out to the `minisign` binary (stubbed here, so no real key is needed).
    def stub_minisign(self, exit_code):
        bin_dir = self.root / "bin"
        bin_dir.mkdir(exist_ok=True)
        stub = bin_dir / "minisign"
        stub.write_text(f"#!/bin/sh\nexit {exit_code}\n")
        stub.chmod(0o755)
        return bin_dir

    def test_real_minisign_adapter_invokes_binary(self):
        self.write_assets(signatures=True)
        key = self.write_key()
        bin_dir = self.stub_minisign(0)
        with mock.patch.dict(os.environ, {"PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}"}):
            code, _, err = self.run_script(
                str(self.assets), VERSION, REPO, "--public-key", str(key)
            )
        self.assertEqual(code, 0, err)
        self.assertTrue((self.assets / "latest.json").exists())

    def test_real_minisign_adapter_reports_nonzero_exit(self):
        self.write_assets(signatures=True)
        key = self.write_key()
        bin_dir = self.stub_minisign(1)
        with mock.patch.dict(os.environ, {"PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}"}):
            code, _, err = self.run_script(
                str(self.assets), VERSION, REPO, "--public-key", str(key)
            )
        self.assertNotEqual(code, 0)
        self.assertIn("signature verification failed", err)
        self.assertFalse((self.assets / "latest.json").exists())



if __name__ == "__main__":
    unittest.main(verbosity=2)
