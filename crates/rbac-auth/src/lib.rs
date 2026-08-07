//! RBAC roles and permissions for management API and mesh ops.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Operator,
    Submitter,
    Reader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    MeshJoin,
    ManageApi,
    SubmitWorkload,
    ReadAsset,
    WriteAsset,
    ManageNodes,
    ViewCluster,
    AuditRead,
    AuditVerify,
    IssueTokens,
}

impl Role {
    pub fn permissions(self) -> HashSet<Permission> {
        use Permission::*;
        match self {
            Role::Admin => [
                MeshJoin,
                ManageApi,
                SubmitWorkload,
                ReadAsset,
                WriteAsset,
                ManageNodes,
                ViewCluster,
                AuditRead,
                AuditVerify,
                IssueTokens,
            ]
            .into_iter()
            .collect(),
            Role::Operator => [
                MeshJoin,
                ManageApi,
                SubmitWorkload,
                ReadAsset,
                WriteAsset,
                ManageNodes,
                ViewCluster,
                AuditRead,
            ]
            .into_iter()
            .collect(),
            Role::Submitter => [SubmitWorkload, ReadAsset, ViewCluster]
                .into_iter()
                .collect(),
            Role::Reader => [ReadAsset, ViewCluster, AuditRead].into_iter().collect(),
        }
    }

    pub fn allows(self, perm: Permission) -> bool {
        self.permissions().contains(&perm)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub name: String,
    pub role: Role,
    /// Optional CAS prefix allow-list (empty = all readable prefixes for role).
    pub asset_prefixes: Vec<String>,
}

impl Principal {
    pub fn admin() -> Self {
        Self {
            name: "admin".into(),
            role: Role::Admin,
            asset_prefixes: vec![],
        }
    }

    pub fn can(&self, perm: Permission) -> bool {
        self.role.allows(perm)
    }

    pub fn can_read_asset(&self, path_or_hash: &str) -> bool {
        if !self.can(Permission::ReadAsset) {
            return false;
        }
        if self.asset_prefixes.is_empty() {
            return true;
        }
        self.asset_prefixes
            .iter()
            .any(|p| path_or_hash.starts_with(p))
    }
}

#[derive(Debug, Clone)]
pub struct RbacError {
    pub message: String,
}

impl std::fmt::Display for RbacError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rbac: {}", self.message)
    }
}

impl std::error::Error for RbacError {}

pub fn require(principal: &Principal, perm: Permission) -> Result<(), RbacError> {
    if principal.can(perm) {
        Ok(())
    } else {
        Err(RbacError {
            message: format!("{:?} denied for role {:?}", perm, principal.role),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_cannot_submit() {
        let p = Principal {
            name: "r".into(),
            role: Role::Reader,
            asset_prefixes: vec![],
        };
        assert!(require(&p, Permission::SubmitWorkload).is_err());
        assert!(require(&p, Permission::ViewCluster).is_ok());
    }
}
