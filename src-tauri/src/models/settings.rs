//! Settings Models
//!
//! System configuration, node types, and Wilaya settings

use serde::{Deserialize, Serialize};

use std::fmt;

/// The type of the node (WILAYA or UNIT)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    #[serde(rename = "WILAYA")]
    Wilaya,
    #[serde(rename = "UNIT")]
    Unit,
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeType::Wilaya => write!(f, "WILAYA"),
            NodeType::Unit => write!(f, "UNIT"),
        }
    }
}

impl std::str::FromStr for NodeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "WILAYA" => Ok(NodeType::Wilaya),
            "UNIT" => Ok(NodeType::Unit),
            _ => Err(format!("Invalid NodeType: {}", s)),
        }
    }
}

/// System settings for WILAYA or UNIT node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub id: i32,
    pub node_type: NodeType,
    pub unit_name: Option<String>,
    pub unit_code: Option<String>,
    /// The currently open fiscal year.
    ///
    /// This is set atomically during `close_year()` and reflects the single
    /// fiscal year that can receive new inventory movements and daily reports.
    /// Access via `SettingsService::get_settings()` or `FiscalYearStatusRepository::get_open_year()`.
    pub current_year: i32,
    pub wilaya_code: Option<String>,
    pub wilaya_name: Option<String>,
    pub configured: bool,
}

impl Settings {
    /// Check if this is a WILAYA node
    pub fn is_wilaya(&self) -> bool {
        self.node_type == NodeType::Wilaya
    }

    /// Check if this is a UNIT node
    pub fn is_unit(&self) -> bool {
        self.node_type == NodeType::Unit
    }

    /// Get display name for the node
    pub fn display_name(&self) -> String {
        match self.node_type {
            NodeType::Wilaya => format!(
                "Wilaya {}",
                self.wilaya_name.as_deref().unwrap_or("Unknown")
            ),
            NodeType::Unit => format!("Unit {}", self.unit_name.as_deref().unwrap_or("Unknown")),
        }
    }

    /// Check if system needs initial configuration
    pub fn needs_configuration(&self) -> bool {
        !self.configured
    }

    /// Get the effective unit identifier
    pub fn get_unit_id(&self) -> Option<&str> {
        if self.is_unit() {
            self.unit_name.as_deref()
        } else {
            None
        }
    }
}

/// Configuration for Wilaya node setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilayaNodeConfiguration {
    pub node_type: NodeType,         // دائماً "WILAYA"
    pub wilaya_code: Option<String>, // كود الولاية (2 أرقام)
    pub wilaya_name: Option<String>, // اسم الولاية
}

impl WilayaNodeConfiguration {
    /// Create a new Wilaya configuration
    pub fn new(wilaya_code: String, wilaya_name: String) -> Self {
        Self {
            node_type: NodeType::Wilaya,
            wilaya_code: Some(wilaya_code),
            wilaya_name: Some(wilaya_name),
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.node_type != NodeType::Wilaya {
            return Err("Node type must be WILAYA".to_string());
        }

        let code = self.wilaya_code.as_ref().ok_or("Wilaya code is required")?;

        if code.len() != 2 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err("Wilaya code must be 2 digits".to_string());
        }

        let name = self.wilaya_name.as_ref().ok_or("Wilaya name is required")?;

        if name.is_empty() {
            return Err("Wilaya name cannot be empty".to_string());
        }

        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            id: 1,
            node_type: NodeType::Unit,
            unit_name: None,
            unit_code: None,
            current_year: 2024,
            wilaya_code: None,
            wilaya_name: None,
            configured: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_node_type() {
        let wilaya = Settings {
            node_type: NodeType::Wilaya,
            wilaya_name: Some("Algiers".to_string()),
            configured: true,
            ..Default::default()
        };
        assert!(wilaya.is_wilaya());
        assert!(!wilaya.is_unit());
        assert!(wilaya.display_name().contains("Wilaya"));
    }

    #[test]
    fn test_wilaya_config_validation() {
        let valid = WilayaNodeConfiguration::new("01".to_string(), "Algiers".to_string());
        assert!(valid.validate().is_ok());

        let invalid_code = WilayaNodeConfiguration::new("1".to_string(), "Algiers".to_string());
        assert!(invalid_code.validate().is_err());

        let invalid_name = WilayaNodeConfiguration::new("01".to_string(), "".to_string());
        assert!(invalid_name.validate().is_err());
    }
}
