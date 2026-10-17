use std::env;
use std::process::Command;

fn main() {
    compatibility_source_digest();
    println!("cargo:rerun-if-env-changed=ARCA_BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=ARCA_BUILD_TYPE");
    println!("cargo:rerun-if-env-changed=ARCA_VERSION");
    println!("cargo:rerun-if-env-changed=AETHER_BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=AETHER_BUILD_TYPE");
    println!("cargo:rerun-if-env-changed=AETHER_VERSION");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let package_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string());
    let version = build_env("BUILD_VERSION")
        .and_then(|value| normalize_gateway_version_source(&value))
        .or_else(|| build_env("VERSION").and_then(|value| normalize_gateway_version_source(&value)))
        .or_else(|| {
            env::var("GITHUB_REF_NAME")
                .ok()
                .and_then(|value| normalize_gateway_version_source(&value))
        })
        .or_else(git_describe_version)
        .filter(|value| !value.is_empty())
        .unwrap_or(package_version);

    println!("cargo:rustc-env=AETHER_BUILD_VERSION={version}");

    let build_type = build_env("BUILD_TYPE")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "source".to_string());
    println!("cargo:rustc-env=AETHER_BUILD_TYPE={build_type}");
}

/// `ARCA_<name>` takes precedence over the legacy `AETHER_<name>`.
fn build_env(name: &str) -> Option<String> {
    env::var(format!("ARCA_{name}"))
        .or_else(|_| env::var(format!("AETHER_{name}")))
        .ok()
}

fn compatibility_source_digest() {
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};
    fn files(path: &Path, out: &mut Vec<PathBuf>) {
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            for entry in std::fs::read_dir(path).expect("compatibility source directory") {
                files(&entry.expect("source entry").path(), out);
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            || path
                .file_name()
                .is_some_and(|n| n == "Cargo.lock" || n == "Cargo.toml")
        {
            out.push(path.to_owned());
        }
    }
    let mut inputs = Vec::new();
    for root in [
        "../../crates/aether-ai/formats",
        "src/api/compatibility",
        "../../Cargo.lock",
        "Cargo.toml",
    ] {
        files(Path::new(root), &mut inputs);
    }
    inputs.sort();
    let mut digest = Sha256::new();
    for path in inputs {
        let bytes = std::fs::read(&path).expect("compatibility source bytes");
        digest.update(path.to_string_lossy().as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    println!(
        "cargo:rustc-env=AFSSR_COMPAT_CONVERTER_DIGEST={:x}",
        digest.finalize()
    );
}

fn git_describe_version() -> Option<String> {
    let output = Command::new("git")
        .args([
            "describe", "--tags", "--match", "v[0-9]*", "--always", "--dirty",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?;
    let version = version.trim();
    normalize_gateway_version_source(version)
}

fn normalize_gateway_version_source(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with("tunnel-v") {
        return None;
    }
    Some(trimmed.strip_prefix('v').unwrap_or(trimmed).to_string())
}
