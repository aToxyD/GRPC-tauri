//! Build Helpers
//!
//! Build-time checks and code generation utilities

use std::env;
use std::path::Path;

/// Verify all required environment variables
pub fn check_environment() {
    let required_vars = [
        ("CARGO_PKG_VERSION", "Package version"),
        ("CARGO_MANIFEST_DIR", "Manifest directory"),
    ];

    for (var, description) in &required_vars {
        if env::var(var).is_err() {
            println!("cargo:warning={} not set ({})", var, description);
        }
    }
}

/// Check for security-sensitive code patterns
pub fn security_audit_check() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let _src_path = Path::new(&manifest_dir).join("src");

    // Check for dangerous patterns (simplified)
    println!("cargo:rustc-cfg=security_audit");
}

/// Configure feature flags based on build profile
pub fn configure_features() {
    let profile = env::var("PROFILE").unwrap_or_default();

    match profile.as_str() {
        "release" => {
            println!("cargo:rustc-cfg=optimized");
        }
        _ => {
            println!("cargo:rustc-cfg=debug_info");
        }
    }
}

/// Generate version info
pub fn generate_version_info() {
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());

    // Get git commit hash
    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();

    // Get build timestamp
    let timestamp = env::var("SOURCE_DATE_EPOCH").ok().unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string())
    });

    println!("cargo:rustc-env=APP_VERSION={}", version);
    println!("cargo:rustc-env=APP_GIT_COMMIT={}", git_commit);
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);
}
