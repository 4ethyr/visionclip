use std::{fs, path::Path};

const CODDY_OWNED_PATHS: &[&str] = &[
    "apps/coddy",
    "apps/coddy-electron",
    "crates/coddy-client",
    "crates/coddy-core",
    "crates/coddy-ipc",
    "crates/coddy-voice-input",
    "crates/voice-input",
    "docs/IMPLEMENTATION_PLAN.md",
    "docs/coddy-architecture-diagram.html",
    "docs/repl",
    "repl_ui",
];

const FORMER_WORKSPACE_MEMBERS: &[&str] = &[
    "apps/coddy",
    "crates/coddy-client",
    "crates/coddy-core",
    "crates/coddy-ipc",
    "crates/coddy-voice-input",
    "crates/voice-input",
];

const DISALLOWED_CODDY_DEPENDENCIES: &[&str] = &["coddy-core", "coddy-ipc", "coddy-voice-input"];
const CODDY_PROTOCOL_DEPENDENCIES: &[&str] = &["coddy-core", "coddy-ipc"];
const CODDY_BRIDGE_SOURCE_PATHS: &[&str] = &["apps/visionclip-daemon/src/coddy_bridge.rs"];

#[test]
fn coddy_owned_project_files_are_not_kept_in_visionclip_repo() {
    let repo_root = repo_root();
    let mut violations = Vec::new();

    for relative_path in CODDY_OWNED_PATHS {
        let path = repo_root.join(relative_path);
        if path.exists() {
            violations.push(format!("{relative_path} still exists in VisionClip"));
        }
    }

    assert!(
        violations.is_empty(),
        "Coddy-owned files must live in the Coddy repository:\n{}",
        violations.join("\n")
    );
}

#[test]
fn coddy_packages_are_not_visionclip_workspace_members() {
    let repo_root = repo_root();
    let manifest_path = repo_root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let mut violations = Vec::new();

    for member in FORMER_WORKSPACE_MEMBERS {
        if manifest.contains(&format!("\"{member}\"")) {
            violations.push(format!("{member} is still a VisionClip workspace member"));
        }
    }

    assert!(
        violations.is_empty(),
        "Coddy packages must not be members of the VisionClip workspace:\n{}",
        violations.join("\n")
    );
}

#[test]
fn visionclip_manifests_do_not_depend_on_coddy_crates() {
    let repo_root = repo_root();
    let mut violations = Vec::new();

    for relative_path in [
        "Cargo.toml",
        "apps/visionclip/Cargo.toml",
        "apps/visionclip-daemon/Cargo.toml",
        "crates/common/Cargo.toml",
    ] {
        let path = repo_root.join(relative_path);
        let manifest = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

        for dependency in DISALLOWED_CODDY_DEPENDENCIES {
            if manifest.contains(dependency) {
                violations.push(format!("{relative_path} depends on {dependency}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "VisionClip manifests must not depend directly on Coddy crates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn visionclip_client_and_common_do_not_depend_on_coddy_crates() {
    let repo_root = repo_root();
    let mut violations = Vec::new();

    for relative_path in ["apps/visionclip/Cargo.toml", "crates/common/Cargo.toml"] {
        let path = repo_root.join(relative_path);
        let manifest = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

        for dependency in DISALLOWED_CODDY_DEPENDENCIES {
            if manifest.contains(dependency) {
                violations.push(format!("{relative_path} depends on {dependency}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "VisionClip client/common must not depend on Coddy crates:\n{}",
        violations.join("\n")
    );
}

#[test]
fn root_workspace_does_not_declare_coddy_protocol_dependencies() {
    let repo_root = repo_root();
    let manifest_path = repo_root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let mut violations = Vec::new();

    for dependency in CODDY_PROTOCOL_DEPENDENCIES {
        if manifest.contains(dependency) {
            violations.push(format!(
                "root Cargo.toml declares {dependency}; keep Coddy protocol paths in the daemon manifest"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "VisionClip root workspace must not declare Coddy protocol dependencies:\n{}",
        violations.join("\n")
    );
}

#[test]
fn daemon_coddy_protocol_dependencies_are_feature_gated() {
    let repo_root = repo_root();
    let manifest_path = repo_root.join("apps/visionclip-daemon/Cargo.toml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let manifest: toml::Value = manifest_text
        .parse()
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", manifest_path.display()));
    let mut violations = Vec::new();

    let features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .expect("daemon manifest should declare features");
    let default_features = features
        .get("default")
        .and_then(toml::Value::as_array)
        .expect("daemon manifest should declare default features");
    if default_features
        .iter()
        .any(|feature| feature.as_str() == Some("coddy-protocol"))
    {
        violations.push(
            "default features must not include coddy-protocol; Coddy integration must be explicit"
                .to_string(),
        );
    }

    let coddy_protocol_features = features
        .get("coddy-protocol")
        .and_then(toml::Value::as_array)
        .expect("daemon manifest should declare coddy-protocol feature");
    if !coddy_protocol_features.is_empty() {
        violations.push(
            "coddy-protocol feature must not enable path dependencies from the VisionClip manifest"
                .to_string(),
        );
    }

    let dependencies = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .expect("daemon manifest should declare dependencies");
    for dependency in CODDY_PROTOCOL_DEPENDENCIES {
        if dependencies.contains_key(*dependency) {
            violations.push(format!(
                "{dependency} must not be a VisionClip manifest dependency; keep Coddy wire compatibility local"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Coddy protocol support must not require sibling path dependencies:\n{}",
        violations.join("\n")
    );
}

#[test]
fn coddy_ipc_source_usage_stays_in_daemon_bridge() {
    let violations = coddy_source_usage_violations("coddy_ipc", CODDY_BRIDGE_SOURCE_PATHS);

    assert!(
        violations.is_empty(),
        "coddy_ipc must stay confined to the daemon Coddy bridge:\n{}",
        violations.join("\n")
    );
}

#[test]
fn coddy_core_source_usage_stays_in_daemon_bridge() {
    let violations = coddy_source_usage_violations("coddy_core", CODDY_BRIDGE_SOURCE_PATHS);

    assert!(
        violations.is_empty(),
        "coddy_core must stay confined to the daemon Coddy bridge:\n{}",
        violations.join("\n")
    );
}

fn coddy_source_usage_violations(marker: &str, allowed_paths: &[&str]) -> Vec<String> {
    let repo_root = repo_root();
    let mut violations = Vec::new();

    for relative_dir in [
        "apps/visionclip/src",
        "apps/visionclip-daemon/src",
        "crates/common/src",
    ] {
        for path in rust_sources_under(&repo_root.join(relative_dir)) {
            let relative_path = path
                .strip_prefix(repo_root)
                .expect("source path under repo root")
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

            if source.contains(marker) && !allowed_paths.contains(&relative_path.as_str()) {
                violations.push(format!(
                    "{relative_path} imports or references {marker} outside the daemon bridge"
                ));
            }
        }
    }

    violations
}

#[test]
fn daemon_main_uses_coddy_bridge_for_repl_contracts() {
    let repo_root = repo_root();
    let relative_path = "apps/visionclip-daemon/src/main.rs";
    let path = repo_root.join(relative_path);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let mut violations = Vec::new();

    for marker in [
        "coddy_core::",
        "ReplCommand::",
        "ReplEvent::",
        "ReplIntent::",
        "resolve_voice_turn_intent",
        "VoiceTurnIntent::",
    ] {
        if source.contains(marker) {
            violations.push(format!("{relative_path} contains direct {marker} usage"));
        }
    }

    assert!(
        violations.is_empty(),
        "Coddy REPL contract details must stay behind coddy_bridge:\n{}",
        violations.join("\n")
    );
}

#[test]
fn coddy_bridge_uses_injected_native_services() {
    let repo_root = repo_root();
    let relative_path = "apps/visionclip-daemon/src/coddy_bridge.rs";
    let path = repo_root.join(relative_path);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let mut violations = Vec::new();

    if source.contains("use super::{process_repl_command") {
        violations.push(format!(
            "{relative_path} imports the daemon REPL pipeline directly"
        ));
    }
    if !source.contains("trait ReplNativeServices") {
        violations.push(format!(
            "{relative_path} must expose ReplNativeServices for daemon injection"
        ));
    }
    if !source.contains("async fn process_repl_command") {
        violations.push(format!(
            "{relative_path} must own the Coddy REPL command pipeline"
        ));
    }

    assert!(
        violations.is_empty(),
        "Coddy bridge must use injected native services:\n{}",
        violations.join("\n")
    );
}

#[test]
fn daemon_main_only_adapts_native_repl_services() {
    let repo_root = repo_root();
    let relative_path = "apps/visionclip-daemon/src/main.rs";
    let path = repo_root.join(relative_path);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let mut violations = Vec::new();

    if source.contains("async fn process_repl_command") {
        violations.push(format!(
            "{relative_path} owns Coddy REPL command orchestration"
        ));
    }
    if !source.contains("impl coddy_bridge::ReplNativeServices for DaemonReplNativeServices") {
        violations.push(format!(
            "{relative_path} must implement the native services adapter for coddy_bridge"
        ));
    }

    assert!(
        violations.is_empty(),
        "Daemon main should adapt native VisionClip services without owning Coddy orchestration:\n{}",
        violations.join("\n")
    );
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("visionclip-common lives under crates/common")
}

fn rust_sources_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut sources = Vec::new();
    let mut pending = vec![dir.to_path_buf()];

    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
            {
                let entry = entry.unwrap_or_else(|error| {
                    panic!("failed to read entry under {}: {error}", path.display())
                });
                pending.push(entry.path());
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }

    sources
}
