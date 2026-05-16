#[derive(Debug, Clone)]
pub enum ResourceContext {
    /// Action does not depend on node/unit scope.
    Global,

    /// Caller is operating as a WILAYA node (built by adapter/usecase).
    WilayaNode,

    /// Caller is operating as a UNIT node with an effective unit_id (built by adapter/usecase).
    UnitNode { unit_id: String },

    /// Read/Write operation scoped to a specific unit_id (resource target).
    UnitScope { unit_id: String },

    /// Operation scoped to a specific user (e.g. read user activity).
    UserScope { user_id: String },
}

impl ResourceContext {
    pub fn unit_id(&self) -> Option<&str> {
        match self {
            ResourceContext::UnitNode { unit_id } => Some(unit_id.as_str()),
            ResourceContext::UnitScope { unit_id } => Some(unit_id.as_str()),
            _ => None,
        }
    }

    pub fn target_user_id(&self) -> Option<&str> {
        match self {
            ResourceContext::UserScope { user_id } => Some(user_id.as_str()),
            _ => None,
        }
    }
}
