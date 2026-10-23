//! Authorization: capability checks resolved from database state.
//!
//! Roles are resolved server-side from team membership, explicit file
//! collaborators and file ownership. Client supplied ids are only used to look
//! rows up, never to grant access.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, AppResult};

/// Ordered capability levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    /// Read the document, comments, versions and activity.
    View,
    /// Create and resolve comments.
    Comment,
    /// Edit the document, versions, assets and shares.
    Edit,
    /// Destructive operations: delete files, manage teams and members.
    Manage,
}

impl Capability {
    /// Human/JSON name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Comment => "comment",
            Self::Edit => "edit",
            Self::Manage => "manage",
        }
    }

    /// Parse a capability name.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "view" => Some(Self::View),
            "comment" => Some(Self::Comment),
            "edit" => Some(Self::Edit),
            "manage" => Some(Self::Manage),
            _ => None,
        }
    }

    /// Everything up to and including this capability.
    pub fn capabilities_upto(self) -> Vec<&'static str> {
        [Self::View, Self::Comment, Self::Edit, Self::Manage]
            .into_iter()
            .filter(|capability| *capability <= self)
            .map(Self::as_str)
            .collect()
    }

    /// Map a role to its maximum capability.
    pub fn for_role(role: Role) -> Self {
        role.capability()
    }
}

/// Effective role on a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Read only.
    Viewer,
    /// Can comment.
    Commenter,
    /// Can edit the document and assets.
    Editor,
    /// Can administer a team but is not its owner.
    Admin,
    /// Team owner.
    Owner,
}

impl Role {
    /// Maximum capability granted by this role.
    pub fn capability(self) -> Capability {
        match self {
            Self::Viewer => Capability::View,
            Self::Commenter => Capability::Comment,
            Self::Editor | Self::Admin | Self::Owner => Capability::Edit,
        }
    }

    /// Capability used for destructive/administrative actions.
    pub fn management_capability(self) -> Capability {
        match self {
            Self::Owner | Self::Admin => Capability::Manage,
            _ => self.capability(),
        }
    }

    /// Role name used in JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Commenter => "commenter",
            Self::Editor => "editor",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }

    fn from_team_role(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(Self::Owner),
            "admin" => Some(Self::Admin),
            "editor" => Some(Self::Editor),
            "viewer" => Some(Self::Viewer),
            _ => None,
        }
    }

    pub(crate) fn from_collaborator_role(raw: &str) -> Option<Self> {
        match raw {
            "edit" => Some(Self::Editor),
            "comment" => Some(Self::Commenter),
            "view" => Some(Self::Viewer),
            _ => None,
        }
    }
}

/// Resolved access to a file.
#[derive(Debug, Clone, Copy)]
pub struct FileAccess {
    /// File id.
    pub file_id: Uuid,
    /// Owning project.
    pub project_id: Uuid,
    /// Owning team.
    pub team_id: Uuid,
    /// Effective role.
    pub role: Role,
}

impl FileAccess {
    /// Effective capabilities.
    pub fn capabilities(&self) -> Vec<&'static str> {
        self.role.capability().capabilities_upto()
    }
}

/// Resolved access to a team.
#[derive(Debug, Clone, Copy)]
pub struct TeamAccess {
    /// Team id.
    pub team_id: Uuid,
    /// Team role.
    pub role: Role,
}

/// Resolved access to a project.
#[derive(Debug, Clone, Copy)]
pub struct ProjectAccess {
    /// Project id.
    pub project_id: Uuid,
    /// Owning team.
    pub team_id: Uuid,
    /// Effective role.
    pub role: Role,
}

#[derive(Debug, sqlx::FromRow)]
struct FileAccessRow {
    project_id: Uuid,
    team_id: Uuid,
    file_created_by: Option<Uuid>,
    team_created_by: Option<Uuid>,
    team_role: Option<String>,
    collaborator_role: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct TeamAccessRow {
    created_by: Option<Uuid>,
    role: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectAccessRow {
    project_id: Uuid,
    team_id: Uuid,
    team_created_by: Option<Uuid>,
    team_role: Option<String>,
}

fn effective_role(
    user_id: Uuid,
    file_created_by: Option<Uuid>,
    team_created_by: Option<Uuid>,
    team_role: Option<&str>,
    collaborator_role: Option<&str>,
) -> Option<Role> {
    let mut role = Role::Viewer;
    let mut found = false;
    let mut consider = |candidate: Role| {
        role = role.max(candidate);
        found = true;
    };
    if team_created_by == Some(user_id) {
        consider(Role::Owner);
    }
    if let Some(raw) = team_role.and_then(Role::from_team_role) {
        consider(raw);
    }
    if let Some(raw) = collaborator_role.and_then(Role::from_collaborator_role) {
        consider(raw);
    }
    if file_created_by == Some(user_id) {
        consider(Role::Editor);
    }
    found.then_some(role)
}

/// Resolve and enforce file access.
pub async fn require_file(
    pool: &PgPool,
    user_id: Uuid,
    file_id: Uuid,
    needed: Capability,
) -> AppResult<FileAccess> {
    let row: Option<FileAccessRow> = sqlx::query_as(
        "SELECT f.project_id, p.team_id, f.created_by AS file_created_by, \
                t.created_by AS team_created_by, tm.role AS team_role, fc.role AS collaborator_role \
         FROM files f \
         JOIN projects p ON p.id = f.project_id \
         JOIN teams t ON t.id = p.team_id \
         LEFT JOIN team_members tm ON tm.team_id = p.team_id AND tm.user_id = $2 \
         LEFT JOIN file_collaborators fc ON fc.file_id = f.id AND fc.user_id = $2 \
         WHERE f.id = $1 AND f.is_deleted = false AND p.is_deleted = false AND t.is_deleted = false",
    )
    .bind(file_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or_else(|| ApiError::not_found("file not found"))?;
    let role = effective_role(
        user_id,
        row.file_created_by,
        row.team_created_by,
        row.team_role.as_deref(),
        row.collaborator_role.as_deref(),
    )
    .ok_or_else(|| ApiError::forbidden("you do not have access to this file"))?;
    check_capability(role, needed)?;
    Ok(FileAccess {
        file_id,
        project_id: row.project_id,
        team_id: row.team_id,
        role,
    })
}

fn check_capability(role: Role, needed: Capability) -> AppResult<()> {
    let granted = match needed {
        Capability::Manage => role.management_capability(),
        _ => role.capability(),
    };
    if granted >= needed {
        Ok(())
    } else {
        Err(ApiError::forbidden_with(
            "capability_required",
            format!(
                "this action requires the `{}` capability (your role grants `{}`)",
                needed.as_str(),
                role.capability().as_str()
            ),
        ))
    }
}

/// Resolve the effective role on a file given the caller's project role and
/// any explicit file collaborator row. Used by list endpoints.
pub fn file_role_from(
    project_role: Role,
    user_id: Uuid,
    file_created_by: Option<Uuid>,
    collaborator_role: Option<&str>,
) -> Role {
    let mut role = project_role;
    if let Some(candidate) = collaborator_role.and_then(Role::from_collaborator_role) {
        role = role.max(candidate);
    }
    if file_created_by == Some(user_id) {
        role = role.max(Role::Editor);
    }
    role
}

/// Resolve and enforce team access.
pub async fn require_team(
    pool: &PgPool,
    user_id: Uuid,
    team_id: Uuid,
    needed: Capability,
) -> AppResult<TeamAccess> {
    let row: Option<TeamAccessRow> = sqlx::query_as(
        "SELECT t.created_by, tm.role \
         FROM teams t \
         LEFT JOIN team_members tm ON tm.team_id = t.id AND tm.user_id = $2 \
         WHERE t.id = $1 AND t.is_deleted = false",
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or_else(|| ApiError::not_found("team not found"))?;
    let role = if row.created_by == Some(user_id) {
        Role::Owner
    } else {
        row.role
            .as_deref()
            .and_then(Role::from_team_role)
            .ok_or_else(|| ApiError::forbidden("you are not a member of this team"))?
    };
    check_capability(role, needed)?;
    Ok(TeamAccess { team_id, role })
}

/// Resolve and enforce project access.
pub async fn require_project(
    pool: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    needed: Capability,
) -> AppResult<ProjectAccess> {
    let row: Option<ProjectAccessRow> = sqlx::query_as(
        "SELECT p.id AS project_id, p.team_id, t.created_by AS team_created_by, tm.role AS team_role \
         FROM projects p \
         JOIN teams t ON t.id = p.team_id \
         LEFT JOIN team_members tm ON tm.team_id = t.id AND tm.user_id = $2 \
         WHERE p.id = $1 AND p.is_deleted = false AND t.is_deleted = false",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or_else(|| ApiError::not_found("project not found"))?;
    let role = if row.team_created_by == Some(user_id) {
        Role::Owner
    } else {
        row.team_role
            .as_deref()
            .and_then(Role::from_team_role)
            .ok_or_else(|| ApiError::forbidden("you do not have access to this project"))?
    };
    check_capability(role, needed)?;
    Ok(ProjectAccess {
        project_id: row.project_id,
        team_id: row.team_id,
        role,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ordering() {
        assert!(Capability::Manage > Capability::Edit);
        assert!(Capability::Edit > Capability::Comment);
        assert!(Capability::Comment > Capability::View);
        assert!(Capability::View < Capability::Comment);
    }

    #[test]
    fn capability_parsing() {
        assert_eq!(Capability::parse("VIEW"), Some(Capability::View));
        assert_eq!(Capability::parse("comment"), Some(Capability::Comment));
        assert_eq!(Capability::parse("edit"), Some(Capability::Edit));
        assert_eq!(Capability::parse("manage"), Some(Capability::Manage));
        assert_eq!(Capability::parse("root"), None);
    }

    #[test]
    fn capability_lists() {
        assert_eq!(Capability::View.capabilities_upto(), vec!["view"]);
        assert_eq!(
            Capability::Edit.capabilities_upto(),
            vec!["view", "comment", "edit"]
        );
        assert_eq!(
            Capability::Manage.capabilities_upto(),
            vec!["view", "comment", "edit", "manage"]
        );
    }

    #[test]
    fn team_roles_map_to_roles() {
        assert_eq!(Role::from_team_role("owner"), Some(Role::Owner));
        assert_eq!(Role::from_team_role("admin"), Some(Role::Admin));
        assert_eq!(Role::from_team_role("editor"), Some(Role::Editor));
        assert_eq!(Role::from_team_role("viewer"), Some(Role::Viewer));
        assert_eq!(Role::from_team_role("editor "), None);
    }

    #[test]
    fn collaborator_roles_map_to_roles() {
        assert_eq!(Role::from_collaborator_role("edit"), Some(Role::Editor));
        assert_eq!(
            Role::from_collaborator_role("comment"),
            Some(Role::Commenter)
        );
        assert_eq!(Role::from_collaborator_role("view"), Some(Role::Viewer));
        assert_eq!(Role::from_collaborator_role("owner"), None);
    }

    #[test]
    fn effective_role_prefers_highest() {
        let user = Uuid::nil();
        // Not a member, but a file collaborator.
        assert_eq!(
            effective_role(user, Some(Uuid::max()), None, None, Some("comment")),
            Some(Role::Commenter)
        );
        // Team editor + file collaborator viewer: editor wins.
        assert_eq!(
            effective_role(user, None, None, Some("editor"), Some("view")),
            Some(Role::Editor)
        );
        // File creator.
        assert_eq!(
            effective_role(user, Some(user), None, Some("viewer"), None),
            Some(Role::Editor)
        );
        // Team owner.
        assert_eq!(
            effective_role(user, None, Some(user), None, None),
            Some(Role::Owner)
        );
        // No membership and no collaborator.
        assert_eq!(effective_role(user, None, None, None, None), None);
    }

    #[test]
    fn capability_checks_are_enforced() {
        assert!(check_capability(Role::Viewer, Capability::View).is_ok());
        assert!(check_capability(Role::Viewer, Capability::Comment).is_err());
        assert!(check_capability(Role::Commenter, Capability::Comment).is_ok());
        assert!(check_capability(Role::Commenter, Capability::Edit).is_err());
        assert!(check_capability(Role::Editor, Capability::Edit).is_ok());
        assert!(check_capability(Role::Editor, Capability::Manage).is_err());
        assert!(check_capability(Role::Admin, Capability::Manage).is_ok());
        assert!(check_capability(Role::Owner, Capability::Manage).is_ok());
    }

    #[test]
    fn roles_serialize_lowercase() {
        assert_eq!(
            serde_json::to_string(&Role::Commenter).expect("serialize"),
            "\"commenter\""
        );
        assert_eq!(Role::Editor.as_str(), "editor");
    }
}
