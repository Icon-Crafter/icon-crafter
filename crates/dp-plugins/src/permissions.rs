//! Declared plugin permissions.
//!
//! Plugins never receive ambient authority: every capability is declared in the
//! manifest, granted by the user, and checked per operation by the sandbox
//! ([`crate::sandbox`]). There is no filesystem, process, or network access in
//! the default host; the optional network permission only records the domains a
//! host *may* allow later — this crate itself never opens a socket.

use serde::{Deserialize, Serialize};

/// Version of the plugin API this host implements.
pub const API_VERSION: u32 = 1;

/// A capability a plugin may request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read document structure and node properties.
    ReadDocument,
    /// Propose document mutations (insert/patch/delete).
    WriteDocument,
    /// Read the current selection.
    ReadSelection,
    /// Change the selection.
    WriteSelection,
    /// Read image/font asset metadata.
    ReadAssets,
    /// Read variables and modes.
    ReadVariables,
    /// Propose variable writes.
    WriteVariables,
    /// Declare external domains the host may allow (not granted automatically).
    Network {
        /// Host names the plugin wants to reach.
        domains: Vec<String>,
    },
    /// Store small JSON values through the host settings mechanism.
    LocalStorage,
}

impl Permission {
    /// Stable machine-readable key.
    pub fn key(&self) -> &'static str {
        match self {
            Permission::ReadDocument => "read_document",
            Permission::WriteDocument => "write_document",
            Permission::ReadSelection => "read_selection",
            Permission::WriteSelection => "write_selection",
            Permission::ReadAssets => "read_assets",
            Permission::ReadVariables => "read_variables",
            Permission::WriteVariables => "write_variables",
            Permission::Network { .. } => "network",
            Permission::LocalStorage => "local_storage",
        }
    }

    /// Whether this capability grants writes.
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            Permission::WriteDocument | Permission::WriteSelection | Permission::WriteVariables
        )
    }

    /// Validates the permission's own payload.
    pub fn validate(&self) -> Result<(), String> {
        if let Permission::Network { domains } = self {
            if domains.is_empty() {
                return Err("network permission needs at least one domain".to_owned());
            }
            for domain in domains {
                if domain.is_empty()
                    || domain.len() > 253
                    || domain.contains(['/', '\\', ':', '*', ' '])
                    || !domain.contains('.')
                {
                    return Err(format!("invalid network domain '{domain}'"));
                }
            }
        }
        Ok(())
    }

    /// Permission keys implied by this one (writes imply their reads).
    pub fn implies(&self) -> &'static [&'static str] {
        match self {
            Permission::WriteDocument => &["read_document"],
            Permission::WriteSelection => &["read_selection"],
            Permission::WriteVariables => &["read_variables"],
            _ => &[],
        }
    }
}

/// The set of permissions a plugin has been granted by the user.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Grants {
    /// Granted permissions, stored as permission keys.
    #[serde(default)]
    pub granted: Vec<String>,
}

impl Grants {
    /// Grants exactly the permissions the manifest declares (the install-time
    /// consent prompt in the UI is responsible for asking the user).
    pub fn from_permissions(permissions: &[Permission]) -> Grants {
        Grants {
            granted: permissions.iter().map(Permission::key).map(str::to_owned).collect(),
        }
    }

    /// Whether a key is granted (writes imply their reads).
    pub fn allows(&self, key: &str) -> bool {
        self.granted.iter().any(|granted| granted == key)
            || self.granted.iter().any(|granted| {
                granted == "write_document" && key == "read_document"
                    || granted == "write_selection" && key == "read_selection"
                    || granted == "write_variables" && key == "read_variables"
            })
    }

    /// Adds a key (used by explicit user grants).
    pub fn insert(&mut self, key: impl Into<String>) {
        let key = key.into();
        if !self.granted.contains(&key) {
            self.granted.push(key);
        }
    }

    /// Removes a key.
    pub fn revoke(&mut self, key: &str) {
        self.granted.retain(|granted| granted != key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_imply_reads() {
        let grants = Grants::from_permissions(&[Permission::WriteDocument]);
        assert!(grants.allows("read_document"));
        assert!(!grants.allows("write_selection"));
    }

    #[test]
    fn revoking_removes_implications() {
        let mut grants = Grants::from_permissions(&[Permission::WriteSelection]);
        assert!(grants.allows("read_selection"));
        grants.revoke("write_selection");
        assert!(!grants.allows("read_selection"));
        assert!(!grants.allows("write_selection"));
    }

    #[test]
    fn network_permission_is_validated() {
        assert!(Permission::Network {
            domains: vec!["api.example.com".to_owned()]
        }
        .validate()
        .is_ok());
        assert!(Permission::Network { domains: vec![] }.validate().is_err());
        assert!(Permission::Network {
            domains: vec!["not a domain".to_owned()]
        }
        .validate()
        .is_err());
        assert!(Permission::Network {
            domains: vec!["https://example.com".to_owned()]
        }
        .validate()
        .is_err());
    }
}
