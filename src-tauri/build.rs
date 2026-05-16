mod build_helpers;

fn main() {
    // Run build helpers
    build_helpers::check_environment();
    build_helpers::security_audit_check();
    build_helpers::configure_features();
    build_helpers::generate_version_info();

    // Run tauri build
    tauri_build::build()
}
