//! 發佈流程：tag 檢查、release notes 擷取，以及 GitHub Actions workflow 的結構。
//! 實際建立 Release 只能在推 tag 後於 GitHub 驗證（見 progress.md 的實機驗收清單）。

use std::path::Path;
use std::process::{Command, Output};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn script(name: &str, args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(Path::new(ROOT).join("packaging").join(name));
    command.args(args).current_dir(ROOT);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn check_tag(tag: &str) -> Output {
    script("check-tag.sh", &[tag], &[])
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn workflow(name: &str) -> String {
    std::fs::read_to_string(Path::new(ROOT).join(".github/workflows").join(name)).unwrap()
}

#[test]
fn check_tag_accepts_the_tag_that_matches_cargo_toml_minor_version() {
    let output = check_tag("v0.2.0");

    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn check_tag_rejects_a_patch_version_tag() {
    let output = check_tag("v0.2.1");

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("PATCH"), "{}", stderr(&output));
}

#[test]
fn check_tag_rejects_a_minor_tag_that_differs_from_cargo_toml() {
    let output = check_tag("v0.3.0");

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("0.2.0"), "{}", stderr(&output));
}

#[test]
fn check_tag_rejects_a_tag_with_extra_dotted_components() {
    let output = check_tag("v0.2.0.0");

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn check_tag_rejects_a_tag_without_the_v_prefix() {
    let output = check_tag("0.2.0");

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn check_tag_exits_2_without_a_tag_argument() {
    let output = script("check-tag.sh", &[], &[]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn release_notes_start_with_the_body_of_the_requested_version() {
    let output = script("release-notes.sh", &["0.2.0"], &[]);

    let notes = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(notes.starts_with("第一個通過實機驗收的版本"), "{notes}");
}

#[test]
fn release_notes_stop_before_the_next_older_version() {
    let output = script("release-notes.sh", &["0.2.0"], &[]);

    let notes = String::from_utf8(output.stdout).unwrap();
    assert!(notes.contains("DeX 視窗裡**鍵盤打不出字"), "{notes}");
    assert!(!notes.contains("第一個測試版"), "{notes}");
    assert!(!notes.contains("## ["), "{notes}");
}

#[test]
fn release_notes_do_not_include_the_unreleased_section() {
    let output = script("release-notes.sh", &["0.2.0"], &[]);

    let notes = String::from_utf8(output.stdout).unwrap();
    assert!(
        !notes.contains("MIN_SCRCPY") && !notes.contains("最低版本由 3.0"),
        "{notes}"
    );
}

#[test]
fn release_notes_fail_for_a_version_missing_from_the_changelog() {
    let output = script("release-notes.sh", &["9.9.9"], &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("9.9.9"), "{}", stderr(&output));
}

#[test]
fn release_notes_fail_for_a_version_prefix_that_only_partly_matches() {
    let output = script("release-notes.sh", &["0.2"], &[]);

    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn release_notes_fail_when_the_version_section_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let changelog = dir.path().join("CHANGELOG.md");
    std::fs::write(
        &changelog,
        "# Changelog\n\n## [0.3.0] - 2026-11-01\n\n## [0.2.0]\n\n內容\n",
    )
    .unwrap();

    let output = script(
        "release-notes.sh",
        &["0.3.0"],
        &[("DEX2PC_CHANGELOG", &changelog)],
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn release_notes_of_the_last_section_run_to_the_end_of_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let changelog = dir.path().join("CHANGELOG.md");
    std::fs::write(
        &changelog,
        "# Changelog\n\n## [0.3.0] - 2026-11-01\n\n最後一段\n",
    )
    .unwrap();

    let output = script(
        "release-notes.sh",
        &["0.3.0"],
        &[("DEX2PC_CHANGELOG", &changelog)],
    );

    assert_eq!(String::from_utf8(output.stdout).unwrap(), "最後一段\n");
}

#[test]
fn release_scripts_are_executable() {
    for name in ["check-tag.sh", "release-notes.sh"] {
        let mode = std::fs::metadata(Path::new(ROOT).join("packaging").join(name))
            .unwrap()
            .permissions();
        assert_ne!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o111,
            0,
            "{name}"
        );
    }
}

#[test]
fn ci_workflow_runs_on_every_push_and_pull_request_and_can_be_reused() {
    let ci = workflow("ci.yml");

    assert!(ci.contains("push:\n    branches: [\"**\"]"), "{ci}");
    assert!(ci.contains("  pull_request:\n"), "{ci}");
    assert!(ci.contains("  workflow_call:\n"), "{ci}");
}

#[test]
fn ci_workflow_does_not_filter_patch_versions() {
    let ci = workflow("ci.yml");

    assert!(!ci.contains("tags:"), "{ci}");
    assert!(!ci.contains("paths:"), "{ci}");
}

#[test]
fn ci_workflow_runs_every_check_and_both_package_builds() {
    let ci = workflow("ci.yml");

    for step in [
        "cargo fmt --check",
        "cargo clippy --all-targets --locked -- -D warnings",
        "cargo test --locked",
        "packaging/build-rpm.sh",
        "packaging/build-deb.sh",
        "lintian --fail-on error dist/*.deb",
        "rhysd/actionlint",
    ] {
        assert!(ci.contains(step), "{step}\n{ci}");
    }
}

#[test]
fn ci_workflow_smoke_tests_the_rpm_and_both_deb_images_from_the_built_artifact() {
    let ci = workflow("ci.yml");

    for command in [
        "packaging/smoke-test.sh",
        "packaging/smoke-test-deb.sh debian:13",
        "packaging/smoke-test-deb.sh ubuntu:26.04",
    ] {
        assert!(
            ci.contains(&format!("command: {command}")),
            "{command}\n{ci}"
        );
    }
    assert!(ci.contains("needs: package"), "{ci}");
    assert!(ci.contains("actions/download-artifact@v4"), "{ci}");
}

#[test]
fn ci_workflow_uploads_rpm_and_deb_as_artifacts() {
    let ci = workflow("ci.yml");

    assert!(ci.contains("actions/upload-artifact@v4"), "{ci}");
    assert!(ci.contains("dist/*.rpm"), "{ci}");
    assert!(ci.contains("dist/*.deb"), "{ci}");
}

#[test]
fn release_workflow_is_triggered_only_by_minor_version_tags() {
    let release = workflow("release.yml");

    assert!(
        release.contains("push:\n    tags: [\"v*.*.0\"]\n"),
        "{release}"
    );
    assert!(!release.contains("branches:"), "{release}");
    assert!(!release.contains("pull_request"), "{release}");
}

#[test]
fn release_workflow_checks_the_tag_before_running_ci_and_ci_before_releasing() {
    let release = workflow("release.yml");

    assert!(
        release.contains("run: packaging/check-tag.sh \"$TAG\""),
        "{release}"
    );
    assert!(
        release.contains("  ci:\n    needs: check-tag\n    uses: ./.github/workflows/ci.yml"),
        "{release}"
    );
    assert!(release.contains("  release:\n    needs: ci\n"), "{release}");
}

#[test]
fn release_workflow_attaches_packages_with_sha256_and_changelog_notes() {
    let release = workflow("release.yml");

    assert!(
        release.contains("sha256sum \"$file\" > \"$file.sha256\""),
        "{release}"
    );
    assert!(
        release.contains("packaging/release-notes.sh \"${TAG#v}\""),
        "{release}"
    );
    assert!(
        release.contains("gh release create \"$TAG\" dist/*"),
        "{release}"
    );
    assert!(
        release.contains("--notes-file release-notes.md"),
        "{release}"
    );
}

#[test]
fn release_workflow_grants_write_permission_only_to_the_release_job() {
    let release = workflow("release.yml");

    assert_eq!(release.matches("contents: write").count(), 1, "{release}");
    assert!(
        release.contains("permissions:\n  contents: read\n"),
        "{release}"
    );
    assert!(
        release.contains("    permissions:\n      contents: write\n"),
        "{release}"
    );
}

#[test]
fn workflows_use_only_the_builtin_token_and_github_hosted_runners() {
    for name in ["ci.yml", "release.yml"] {
        let text = workflow(name);
        let secrets: Vec<&str> = text.matches("secrets.").collect();

        assert_eq!(
            secrets.len(),
            text.matches("secrets.GITHUB_TOKEN").count(),
            "{name}: 只能用 GITHUB_TOKEN"
        );
        assert!(!text.contains("self-hosted"), "{name}");
        assert!(
            !text.contains("runs-on: [") && !text.contains("runs-on: ${{"),
            "{name}"
        );
        for line in text.lines().filter(|line| line.contains("runs-on:")) {
            assert_eq!(line.trim(), "runs-on: ubuntu-latest", "{name}");
        }
    }
}

#[test]
fn workflows_pass_actionlint_when_it_is_installed() {
    let Ok(output) = Command::new("actionlint").current_dir(ROOT).output() else {
        eprintln!("沒有安裝 actionlint，略過（CI 的 workflow-lint job 會跑）");
        return;
    };

    assert!(output.status.success(), "{output:?}");
}

#[test]
fn rpm_spec_pins_doc_dir_naming_so_ubuntu_built_rpms_match_fedora_ones() {
    // CI 在 Ubuntu 上用 apt 的 rpmbuild：預設文件目錄會帶版號、還會多出 /usr/lib/.build-id
    let spec = std::fs::read_to_string(Path::new(ROOT).join("packaging/rpm/dex2pc.spec")).unwrap();

    assert!(spec.contains("%global _docdir_fmt %{name}\n"), "{spec}");
    assert!(spec.contains("%global _build_id_links none\n"), "{spec}");
}

#[test]
fn rpm_smoke_test_finds_an_rpm_that_has_no_dist_tag_in_its_name() {
    // Ubuntu 的 rpmbuild 沒有 %dist，檔名是 dex2pc-<版號>-1.x86_64.rpm
    let smoke = std::fs::read_to_string(Path::new(ROOT).join("packaging/smoke-test.sh")).unwrap();

    assert!(
        smoke.contains("\"dist/dex2pc-$version-1\"*.x86_64.rpm"),
        "{smoke}"
    );
}
