"""Exercise macOS secret gating/failure handling without a Mac or real credentials."""
import base64
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "packaging/macos/release.sh"
MOCK = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ['CALL_LOG'], 'a') as f:
    f.write(json.dumps([name, args, os.environ.get('MACOS_SIGN_IDENTITY'), os.environ.get('MACOS_INSTALLER_SIGN_IDENTITY')]) + '\n')
if name == 'uname': print('arm64')
elif name == 'cargo':
    pathlib.Path('target/macos-package/Local Image.app').mkdir(parents=True)
    pathlib.Path('dist/release').mkdir(parents=True)
    pathlib.Path('dist/release/local-image-2.0.0-alpha.1-macos-arm64.pkg').write_text('fixture')
elif name == 'security':
    if args[0] == 'create-keychain': pathlib.Path(args[-1]).touch()
    if args[0] == 'delete-keychain': pathlib.Path(args[-1]).unlink()
elif name == 'ditto':
    if '-k' in args: pathlib.Path(args[-1]).write_text('fixture')
    else: pathlib.Path(args[-1]).mkdir(parents=True)
elif name == 'hdiutil': pathlib.Path(args[-1]).write_text('fixture')
elif name == 'notarytool' and os.environ.get('FAIL_NOTARY'): sys.exit(42)
elif name == 'xcrun' and args[0] == 'notarytool' and os.environ.get('FAIL_NOTARY'): sys.exit(42)
'''


class MacReleaseTests(unittest.TestCase):
    def run_release(self, secrets=None):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        tools = root / "bin"
        tools.mkdir()
        for name in ["uname", "cargo", "security", "ditto", "hdiutil", "codesign", "xcrun", "spctl"]:
            p = tools / name
            p.write_text(MOCK)
            p.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if not k.startswith("MACOS_") and k not in ["CARGO_TARGET_DIR", "DIST", "RELEASE_REQUIRE_SIGNING"]}
        env.update(PATH=f"{tools}:{env['PATH']}", RUNNER_TEMP=str(root), CALL_LOG=str(root / "calls.jsonl"))
        env.update(secrets or {})
        result = subprocess.run(["bash", str(SCRIPT)], cwd=root, env=env, capture_output=True, text=True)
        import json
        calls = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
        return root, result, calls

    def test_missing_or_partial_secrets_skip_signing_cleanly(self):
        for secrets in [{}, {"MACOS_SIGN_IDENTITY": "fake-identity"}]:
            root, result, calls = self.run_release(secrets)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("signing/notarization skipped", result.stdout)
            self.assertFalse(any(c[0] in ["security", "codesign", "xcrun"] for c in calls))
            cargo = next(c for c in calls if c[0] == "cargo")
            self.assertEqual(cargo[2:], [None, None])
            self.assertEqual(cargo[1], ["+1.98.1", "xtask", "package", "macos"])
            self.assertTrue(list((root / "dist/release").glob("*.dmg")))
            self.assertTrue(list((root / "dist/release").glob("*.app.zip.sha256")))

    @staticmethod
    def secrets():
        return dict(MACOS_CERTIFICATE=base64.b64encode(b"fake-certificate").decode(), MACOS_CERTIFICATE_PASSWORD="fake-password",
                    MACOS_KEYCHAIN_PASSWORD="fake-keychain", MACOS_SIGN_IDENTITY="fake-app", MACOS_INSTALLER_SIGN_IDENTITY="fake-installer",
                    MACOS_NOTARY_APPLE_ID="fake@example.invalid", MACOS_NOTARY_TEAM_ID="FAKE", MACOS_NOTARY_PASSWORD="fake-notary")

    def test_complete_secrets_sign_notarize_staple_and_remove_key_material(self):
        root, result, calls = self.run_release(self.secrets())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum(c[0] == "xcrun" and c[1][:2] == ["notarytool", "submit"] for c in calls), 3)
        self.assertEqual(sum(c[0] == "xcrun" and c[1][:2] == ["stapler", "validate"] for c in calls), 3)
        self.assertFalse((root / "local-image-signing.p12").exists())
        self.assertFalse((root / "local-image-signing.keychain-db").exists())
        self.assertTrue(any(c[0] == "security" and c[1][0] == "delete-keychain" for c in calls))

    def test_tag_release_without_signing_secrets_fails_before_building(self):
        for secrets in [{}, {"MACOS_SIGN_IDENTITY": "fake-identity"}]:
            root, result, calls = self.run_release({**secrets, "RELEASE_REQUIRE_SIGNING": "1"})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("refusing to build an ad-hoc release", result.stderr)
            self.assertFalse(any(c[0] in ["cargo", "hdiutil", "codesign", "xcrun"] for c in calls))
            self.assertFalse((root / "dist").exists())

    def test_tag_release_without_notarization_secrets_fails_before_building(self):
        secrets = {k: v for k, v in self.secrets().items() if not k.startswith("MACOS_NOTARY_")}
        root, result, calls = self.run_release({**secrets, "RELEASE_REQUIRE_SIGNING": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing to build an un-notarized release", result.stderr)
        self.assertFalse(any(c[0] in ["cargo", "hdiutil", "xcrun"] for c in calls))
        self.assertFalse((root / "local-image-signing.p12").exists())
        self.assertFalse((root / "local-image-signing.keychain-db").exists())

    def test_tag_release_with_complete_secrets_still_signs_and_notarizes(self):
        root, result, calls = self.run_release({**self.secrets(), "RELEASE_REQUIRE_SIGNING": "1"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum(c[0] == "xcrun" and c[1][:2] == ["notarytool", "submit"] for c in calls), 3)

    def test_configured_notarization_failure_is_fatal_and_cleans_keys(self):
        secrets = self.secrets()
        secrets["FAIL_NOTARY"] = "1"
        root, result, calls = self.run_release(secrets)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(c[0] == "hdiutil" for c in calls))
        self.assertFalse((root / "local-image-signing.p12").exists())
        self.assertFalse((root / "local-image-signing.keychain-db").exists())


if __name__ == "__main__":
    unittest.main()
