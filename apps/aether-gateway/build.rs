use std::env;
use std::process::Command;

fn main() {
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
