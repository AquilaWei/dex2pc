"""讓 harness 的 pytest 實際跑 Rust 專案的檢查指令（與 feature_list.json 的 verify 相同）。"""

import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def run_cargo(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["cargo", *args], cwd=REPO_ROOT, capture_output=True, text=True, check=False
    )


def test_cargo_fmt_check_reports_no_diff():
    result = run_cargo("fmt", "--check")
    assert result.returncode == 0, result.stdout + result.stderr


def test_cargo_clippy_reports_no_warnings():
    result = run_cargo("clippy", "--all-targets", "--", "-D", "warnings")
    assert result.returncode == 0, result.stderr


def test_cargo_test_passes():
    result = run_cargo("test")
    assert result.returncode == 0, result.stdout + result.stderr
