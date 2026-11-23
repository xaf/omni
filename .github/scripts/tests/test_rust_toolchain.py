"""Exercise compiler selection and change filtering offline."""

import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
SCRIPTS = ROOT / ".github/scripts"


class ToolchainPolicyTests(unittest.TestCase):
    def test_ci_reads_the_proposed_compiler_instead_of_a_hardcoded_version(self):
        with tempfile.TemporaryDirectory() as directory:
            pin = Path(directory) / "rust-toolchain.toml"
            for version in ["1.99.1", "1.100.0"]:
                pin.write_text(f'[toolchain]\nchannel = "{version}"\n')
                result = subprocess.run(
                    ["python3", str(SCRIPTS / "rust-toolchain.py")],
                    cwd=directory, capture_output=True, text=True, check=True,
                )
                self.assertEqual(result.stdout.strip(), version)
            for contents in ["", "not toml", "[toolchain]\nchannel = ''"]:
                pin.write_text(contents)
                result = subprocess.run(
                    ["python3", str(SCRIPTS / "rust-toolchain.py")],
                    cwd=directory, capture_output=True,
                )
                self.assertNotEqual(result.returncode, 0)

    def test_toolchain_only_pr_triggers_rust_checks(self):
        workflow = (ROOT / ".github/workflows/tests.yaml").read_text()
        env = os.environ.copy()
        for name in ["CORE", "TOOLCHAIN"]:
            env[f"MODIFIED_FILES_{name}"] = re.search(
                rf"^\s+MODIFIED_FILES_{name}: (.+)$", workflow, re.MULTILINE
            ).group(1)
        with tempfile.TemporaryDirectory() as directory:
            def git(*args):
                return subprocess.check_output(
                    ["git", "-c", "user.name=Test", "-c", "user.email=test@example.com", *args],
                    cwd=directory, text=True,
                ).strip()

            git("init", "-q")
            pin = Path(directory) / "rust-toolchain.toml"
            pin.write_text('[toolchain]\nchannel = "1.99.1"\n')
            git("add", ".")
            git("commit", "-qm", "Initial pin")
            env["BASE_SHA"] = git("rev-parse", "HEAD")
            env["GITHUB_OUTPUT"] = os.devnull
            for filename in ["rust-toolchain.toml", "rust-toolchain"]:
                with self.subTest(filename=filename):
                    git("reset", "--hard", "HEAD")
                    if filename == "rust-toolchain":
                        git("mv", "rust-toolchain.toml", filename)
                    (Path(directory) / filename).write_text(
                        '[toolchain]\nchannel = "1.100.0"\n'
                    )
                    result = subprocess.run(
                        ["bash", str(SCRIPTS / "get-changed-files.sh")],
                        cwd=directory, env=env, capture_output=True, text=True, check=True,
                    )
                    self.assertIn("core_any_modified=true", result.stdout)
                    self.assertIn("toolchain_any_modified=true", result.stdout)


if __name__ == "__main__":
    unittest.main()
