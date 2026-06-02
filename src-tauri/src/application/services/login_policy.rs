//! Login Policy — authorization rules enforced during login.
//!
//! ARCHITECTURE: Splits authorization decisions from authentication logic
//! so that access policies are testable, governable, and extensible
//! without modifying the login command.

use crate::errors::BusinessLogicError;
use crate::models::{NodeType, UserRole};

/// Login authorization policy.
///
/// Encapsulates all rules that determine whether an authenticated user
/// is permitted to access the system from a given operational context.
pub struct LoginPolicy;

impl LoginPolicy {
    /// Check whether the user role is permitted to log in from a node
    /// with the given type.
    ///
    /// # Errors
    /// Returns `BusinessLogicError::OperationNotPermitted` if the
    /// combination is not allowed.
    pub fn check_login_allowed(
        node_type: &NodeType,
        user_role: &UserRole,
    ) -> Result<(), BusinessLogicError> {
        // Wilaya nodes restrict login to Admin users only.
        if *node_type == NodeType::Wilaya && *user_role == UserRole::User {
            return Err(BusinessLogicError::OperationNotPermitted {
                message: "المستخدم العادي غير مصرح له بتسجيل الدخول في عقدة الولاية".to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilaya_allows_admin() {
        assert!(LoginPolicy::check_login_allowed(&NodeType::Wilaya, &UserRole::Admin).is_ok());
    }

    #[test]
    fn wilaya_rejects_regular_user() {
        assert!(LoginPolicy::check_login_allowed(&NodeType::Wilaya, &UserRole::User).is_err());
    }

    #[test]
    fn unit_allows_any_role() {
        assert!(LoginPolicy::check_login_allowed(&NodeType::Unit, &UserRole::User).is_ok());
        assert!(LoginPolicy::check_login_allowed(&NodeType::Unit, &UserRole::Admin).is_ok());
    }
}
