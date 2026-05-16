use crate::models::UserRole;

#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
    pub session_id: Option<String>,
}
