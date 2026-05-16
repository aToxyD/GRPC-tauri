//! Ensure user-facing messages are stable for sync/idempotency errors.

use grpc_lib::errors::{into_command_error, AppError, BusinessLogicError};

#[test]
fn duplicate_sync_package_is_user_friendly() {
    let e = AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage {
        package_id: "pkg-dup-001".into(),
    });
    let msg = into_command_error(e);
    assert!(
        msg.contains("تم استيراد هذه الحزمة مسبقاً"),
        "unexpected message: {}",
        msg
    );
}
