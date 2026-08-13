//! Licensing enforcement gate end-to-end tests (ADR-0042 §5).
//!
//! These exercise the REAL command authorization path (`authorize_command`)
//! which runs the licensing gate, and the REAL services for anchor install and
//! license import. The node's public key (subject binding) is resolved by the
//! command layer from `NodeKeyStore` under `dirs::data_dir()`; the binding
//! tests pin `XDG_DATA_HOME` to a per-test temp dir (serialized by a process
//! mutex) so the node key store is hermetic.

#[allow(dead_code)]
mod common;

use std::sync::Mutex;

use grpc_lib::application::authz::Action;
use grpc_lib::application::licensing::trust_anchor_service::TrustAnchorService;
use grpc_lib::commands::{authorize_command, AppState};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::errors::{AppError, AuthorizationError};
use grpc_lib::infrastructure::identity::NodeKeyStore;

const NODE_SECRET: [u8; 32] = [42u8; 32];
const OTHER_NODE_SECRET: [u8; 32] = [7u8; 32];
const LICENSOR_SECRET: [u8; 32] = [99u8; 32];
const ANCHOR_KEY_ID: &str = "lk-e2e-0001";

/// Serializes tests that mutate `XDG_DATA_HOME`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn wilaya_state() -> AppState {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'TestWilaya' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn set_session(state: &AppState, role: &str) {
    let mut session = common::create_test_session("u1", "admin", role);
    session.user_role = grpc_lib::models::UserRole::from(role.to_string());
    common::insert_test_user(state, "u1", "admin", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

fn install_anchor(state: &AppState) -> String {
    let package = provisioning_package();
    let mut guard = state.get_db().expect("lock");
    let db = guard.as_mut().expect("db");
    TrustAnchorService::import_anchor(db, &package).unwrap();
    package
}

fn import_license(state: &AppState, artifact: &str, import_node_key: Option<Vec<u8>>) -> String {
    let mut guard = state.get_db().expect("lock");
    let db = guard.as_mut().expect("db");
    grpc_lib::application::licensing::LicenseVerificationService::import_license(
        db,
        import_node_key,
        artifact,
    )
    .unwrap()
    .outcome
}

// ── artifact/provisioning fixtures (built with raw ed25519-dalek — the
//    library exposes no production signing ability) ────────────────────────

fn provisioning_package() -> String {
    let public_key = ed25519_dalek::SigningKey::from_bytes(&LICENSOR_SECRET)
        .verifying_key()
        .to_bytes()
        .to_vec();
    String::from_utf8(
        grpc_lib::infrastructure::licensing::provisioning::encode_provisioning_package(
            ANCHOR_KEY_ID,
            &public_key,
        )
        .unwrap(),
    )
    .unwrap()
}

fn node_public_key(secret: &[u8; 32]) -> Vec<u8> {
    ed25519_dalek::SigningKey::from_bytes(secret)
        .verifying_key()
        .to_bytes()
        .to_vec()
}

fn subject_id(secret: &[u8; 32]) -> String {
    grpc_lib::infrastructure::licensing::encoding::encode_base64url(&node_public_key(secret))
}

/// Canonical signed artifact, signed by the licensor key, bound to `subject_id`.
fn signed_artifact(subject_id: &str, entitlements: &[&str], status: &str) -> String {
    use ed25519_dalek::Signer as _;
    use grpc_lib::infrastructure::licensing::artifact_io::{
        derive_artifact_id, encode, encode_unsigned,
    };
    use grpc_lib::infrastructure::licensing::encoding::encode_base64url;
    use grpc_lib::models::*;
    use sha2::{Digest, Sha256};

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&LICENSOR_SECRET);
    let payload = ArtifactPayload {
        license: LicensePayload {
            id: "lic-e2e-0001".into(),
            type_key: "production".into(),
            status: status.into(),
        },
        subject: SubjectPayload {
            id: subject_id.to_string(),
        },
        entitlements: entitlements.iter().map(|e| e.to_string()).collect(),
        contract_version: ContractVersionPayload { major: 1, minor: 0 },
    };
    let artifact_id = derive_artifact_id(1, ANCHOR_KEY_ID, subject_id, &payload).unwrap();
    let metadata = ArtifactMetadata {
        key_id: ANCHOR_KEY_ID.into(),
        algorithm: "Ed25519".into(),
        artifact_id,
        issued_for: subject_id.to_string(),
    };
    let unsigned = encode_unsigned(1, &metadata, &payload).unwrap();
    let digest = Sha256::digest(&unsigned);
    let signature = signing_key.sign(&digest).to_bytes().to_vec();
    let artifact = SignedLicenseArtifact {
        version: 1,
        metadata,
        payload,
        signature: ArtifactSignatureRef {
            algorithm: "Ed25519".into(),
            key_id: ANCHOR_KEY_ID.into(),
            signature: encode_base64url(&signature),
        },
    };
    String::from_utf8(encode(&artifact).unwrap()).unwrap()
}

/// Writes the node signing key to `XDG_DATA_HOME/GRPC` so the command layer's
/// `resolve_node_public_key()` resolves it.
fn pin_node_key(dir: &std::path::Path) {
    let store = NodeKeyStore::new(dir.join("GRPC"));
    store.write(&NODE_SECRET).unwrap();
}

fn expect_license_error(err: AppError, variant: &str) {
    match err {
        AppError::Authorization(e) => match variant {
            "LicenseRequired" => assert!(matches!(e, AuthorizationError::LicenseRequired)),
            "EntitlementRequired" => {
                assert!(matches!(e, AuthorizationError::EntitlementRequired { .. }))
            }
            "LicenseNotForThisNode" => {
                assert!(matches!(e, AuthorizationError::LicenseNotForThisNode))
            }
            _ => panic!("unexpected variant {variant}: {e:?}"),
        },
        e => panic!("expected authorization error, got {e:?}"),
    }
}

// ── tests: dormant gate (no anchor) ───────────────────────────────────────

#[test]
fn e2e_gate_is_dormant_without_anchor() {
    let state = wilaya_state();
    set_session(&state, "Admin");
    // Non-exempt stock action still allowed while dormant.
    authorize_command(&state, Action::ManageOrders, None).expect("dormant gate allows");
    // And without any node key present.
    authorize_command(&state, Action::ManageProducts, None).expect("dormant gate allows");
}

// ── tests: active gate, no license ────────────────────────────────────────

#[test]
fn e2e_gate_denies_without_license_when_anchor_installed() {
    let state = wilaya_state();
    install_anchor(&state);
    set_session(&state, "Admin");
    let err = authorize_command(&state, Action::ManageOrders, None).expect_err("deny");
    expect_license_error(err, "LicenseRequired");
}

#[test]
fn e2e_gate_denies_export_products_without_license() {
    let state = wilaya_state();
    install_anchor(&state);
    set_session(&state, "Admin");
    let err = authorize_command(&state, Action::ExportProducts, None).expect_err("deny");
    expect_license_error(err, "LicenseRequired");
}

// ── tests: exempt actions stay available when the gate is active ──────────

#[test]
fn e2e_licensing_management_is_exempt_when_gate_active() {
    let state = wilaya_state();
    set_session(&state, "Admin");
    install_anchor(&state);
    // Status is readable (Wilaya nodes gate all reads to Admin).
    authorize_command(&state, Action::ReadLicensingStatus, None).expect("exempt read");
    // Management requires admin but is exempt from the gate.
    authorize_command(&state, Action::ManageLicensing, None).expect("exempt admin");
    // Audit observability remains available.
    authorize_command(&state, Action::ReadAuditLog, None).expect("exempt audit");
}

#[test]
fn e2e_licensing_status_readable_by_unit_user_when_gate_active() {
    // A UNIT node lets any authenticated user read licensing status.
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-scope-id', wilaya_code = '16', wilaya_name = NULL WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    set_session(&state, "User");
    install_anchor(&state);
    authorize_command(&state, Action::ReadLicensingStatus, None).expect("unit user may read");
}

#[test]
fn e2e_non_admin_cannot_manage_licensing() {
    let state = wilaya_state();
    set_session(&state, "User");
    let err = authorize_command(&state, Action::ManageLicensing, None).expect_err("deny");
    match err {
        AppError::Authorization(AuthorizationError::RequiresAdmin) => {}
        e => panic!("expected RequiresAdmin, got {e:?}"),
    }
}

// ── tests: active gate with a node key (binding evaluated) ─────────────────

#[test]
fn e2e_gate_allows_when_bound_license_grants_entitlement() {
    let _lock = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.path());
    pin_node_key(dir.path());

    let state = wilaya_state();
    install_anchor(&state);
    let artifact = signed_artifact(&subject_id(&NODE_SECRET), &["core.stock"], "active");
    assert_eq!(
        import_license(&state, &artifact, Some(node_public_key(&NODE_SECRET))),
        "imported"
    );
    set_session(&state, "Admin");

    authorize_command(&state, Action::ManageOrders, None).expect("bound license grants stock");
    authorize_command(&state, Action::ManageProducts, None).expect("bound license grants stock");
    // A non-stock action is still denied.
    let err = authorize_command(&state, Action::ManageDailyReports, None).expect_err("deny");
    expect_license_error(err, "EntitlementRequired");
}

#[test]
fn e2e_gate_denies_when_bound_license_lacks_entitlement() {
    let _lock = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.path());
    pin_node_key(dir.path());

    let state = wilaya_state();
    install_anchor(&state);
    let artifact = signed_artifact(&subject_id(&NODE_SECRET), &["core.reports"], "active");
    assert_eq!(
        import_license(&state, &artifact, Some(node_public_key(&NODE_SECRET))),
        "imported"
    );
    set_session(&state, "Admin");

    let err = authorize_command(&state, Action::ManageOrders, None).expect_err("deny");
    expect_license_error(err, "EntitlementRequired");
    // The license does grant reports.
    authorize_command(&state, Action::ManageDailyReports, None).expect("reports granted");
}

#[test]
fn e2e_gate_denies_license_bound_to_another_node() {
    let _lock = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.path());
    pin_node_key(dir.path());

    let state = wilaya_state();
    install_anchor(&state);
    // A valid license bound to another node's key persists (it is Bound to
    // THAT node) and stays stored. Once this node's key differs from the
    // license subject (rotation / migration), the gate must fail-closed with
    // `LicenseNotForThisNode`.
    let artifact = signed_artifact(&subject_id(&OTHER_NODE_SECRET), &["core.stock"], "active");
    assert_eq!(
        import_license(&state, &artifact, Some(node_public_key(&OTHER_NODE_SECRET))),
        "imported"
    );
    set_session(&state, "Admin");

    let err = authorize_command(&state, Action::ManageOrders, None).expect_err("deny");
    expect_license_error(err, "LicenseNotForThisNode");
}

#[test]
fn e2e_gate_denies_when_node_key_missing_after_anchor() {
    // No node key pinned: even a perfectly signed license cannot be bound.
    let _lock = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.path());

    let state = wilaya_state();
    install_anchor(&state);
    set_session(&state, "Admin");
    let err = authorize_command(&state, Action::ManageOrders, None).expect_err("deny");
    expect_license_error(err, "LicenseRequired");
}
