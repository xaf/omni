use super::*;

fn detect(files: &[(&str, &str)]) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).unwrap();
    }
    detect_version_from_rust_toolchain("rust".into(), dir.path().into())
}

#[test]
fn rust_toolchain_reads_toml_channel_and_ignores_install_settings() {
    assert_eq!(
        detect(&[(
            "rust-toolchain.toml",
            r#"
                [toolchain]
                channel = "1.99.1"
                components = ["clippy", "rustfmt"]
                targets = ["aarch64-apple-darwin"]
                profile = "minimal"
            "#,
        )]),
        Some("1.99.1".into())
    );
}

#[test]
fn rust_toolchain_supports_both_legacy_formats() {
    for contents in [
        "  1.99.1\n",
        "[toolchain]\nchannel = '1.99.1'\n",
        "toolchain = { channel = '1.99.1' }",
    ] {
        assert_eq!(
            detect(&[("rust-toolchain", contents)]),
            Some("1.99.1".into())
        );
    }
}

#[test]
fn rust_toolchain_legacy_filename_takes_precedence_like_rustup() {
    assert_eq!(
        detect(&[
            ("rust-toolchain", "1.98.0"),
            ("rust-toolchain.toml", "[toolchain]\nchannel = '1.99.1'"),
        ]),
        Some("1.98.0".into())
    );
    // An invalid legacy file must not silently select the other file.
    assert_eq!(
        detect(&[
            ("rust-toolchain", "nightly"),
            ("rust-toolchain.toml", "[toolchain]\nchannel = '1.99.1'"),
        ]),
        None
    );
}

#[test]
fn rust_toolchain_rejects_invalid_and_unsupported_inputs() {
    for contents in [
        "",
        "1.99.1",
        "[toolchain",
        "channel = '1.99.1'",
        "[toolchain]\ncomponents = ['clippy']",
        "[toolchain]\nchannel = 199",
        "[toolchain]\nchannel = ''",
        "[toolchain]\nchannel = '1.99'",
        "[toolchain]\nchannel = 'stable'",
        "[toolchain]\nchannel = 'beta'",
        "[toolchain]\nchannel = 'nightly-2026-11-01'",
        "[toolchain]\nchannel = '1.100.0-beta.1'",
        "[toolchain]\nchannel = '1.99.1+custom'",
        "[toolchain]\nchannel = '1.99.1-x86_64-unknown-linux-gnu'",
        "[toolchain]\nchannel = '>=1.99.1'",
        "[toolchain]\nchannel = ' 1.99.1 '",
        "[toolchain]\npath = '/custom/toolchain'",
        "[toolchain]\nchannel = '1.99.1'\npath = '/custom/toolchain'",
    ] {
        assert_eq!(
            detect(&[("rust-toolchain.toml", contents)]),
            None,
            "accepted unsupported toolchain: {contents:?}"
        );
    }
    for contents in ["", "stable", "1.99.1\n1.98.0", "custom", "1.99.1 # comment"] {
        assert_eq!(detect(&[("rust-toolchain", contents)]), None);
    }
}

#[test]
fn rust_toolchain_missing_non_file_and_non_utf8_inputs_are_ignored() {
    assert_eq!(detect(&[]), None);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("rust-toolchain.toml");
    std::fs::create_dir(&file).unwrap();
    assert_eq!(
        detect_version_from_rust_toolchain("rust".into(), dir.path().into()),
        None
    );
    std::fs::remove_dir(&file).unwrap();
    std::fs::write(&file, [0xff, 0xfe]).unwrap();
    assert_eq!(
        detect_version_from_rust_toolchain("rust".into(), dir.path().into()),
        None
    );
}

#[test]
fn rust_toolchain_auto_detection_precedence_and_tool_isolation() {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("rust-toolchain.toml", "[toolchain]\nchannel = '1.99.1'"),
        (".rust-version", "1.98.0"),
        (".tool-versions", "rust 1.97.0"),
    ] {
        std::fs::write(dir.path().join(name), contents).unwrap();
    }
    let config = UpConfigMise::new("rust", "auto", BTreeSet::new(), false);
    assert_eq!(
        config.detect_version_at_path("rust", dir.path()),
        Some("1.99.1".into())
    );
    assert_eq!(
        detect_version_from_rust_toolchain("python".into(), dir.path().into()),
        None
    );

    // Existing per-tool detectors retain their priority.
    let mut custom = config.clone();
    custom.add_detect_version_func(|_, _| Some("1.96.0".into()));
    assert_eq!(
        custom.detect_version_at_path("rust", dir.path()),
        Some("1.96.0".into())
    );

    // A configured version bypasses all file detection, including callbacks.
    for version in ["1.95.0", "latest", "^1.94.0"] {
        let mut explicit = UpConfigMise::new("rust", version, BTreeSet::new(), false);
        explicit.add_detect_version_func(|_, _| panic!("explicit version consulted files"));
        assert_eq!(
            explicit.detect_version_at_path("rust", dir.path()),
            Some(version.into())
        );
    }

    std::fs::remove_file(dir.path().join("rust-toolchain.toml")).unwrap();
    std::fs::remove_file(dir.path().join(".tool-versions")).unwrap();
    assert_eq!(
        config.detect_version_at_path("rust", dir.path()),
        Some("1.98.0".into())
    );
}
