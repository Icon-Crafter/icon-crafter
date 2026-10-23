//! Shared response models.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Public user representation (never includes the password hash).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserView {
    /// User id.
    pub id: Uuid,
    /// Normalized e-mail.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Team summary with the caller's role.
#[derive(Debug, Clone, Serialize)]
pub struct TeamSummary {
    /// Team id.
    pub id: Uuid,
    /// Team name.
    pub name: String,
    /// Caller role on the team.
    pub role: String,
    /// Number of members.
    pub member_count: i64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Team member entry.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TeamMemberView {
    /// User id.
    pub user_id: Uuid,
    /// Normalized e-mail.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Team role.
    pub role: String,
    /// Membership creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Project summary.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectView {
    /// Project id.
    pub id: Uuid,
    /// Owning team.
    pub team_id: Uuid,
    /// Project name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// File summary with version metadata.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FileView {
    /// File id.
    pub id: Uuid,
    /// Owning project.
    pub project_id: Uuid,
    /// File name.
    pub name: String,
    /// Branch parent when this file is a branch.
    pub branch_of: Option<Uuid>,
    /// Creating user.
    pub created_by: Option<Uuid>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Head version id.
    pub head_version_id: Option<Uuid>,
    /// Caller role on the file.
    pub role: String,
}

/// Version list entry.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VersionSummary {
    /// Version id.
    pub id: Uuid,
    /// File id.
    pub file_id: Uuid,
    /// Version label (empty for automatic snapshots).
    pub label: String,
    /// `snapshot`, `delta`, `named` or `branch_base`.
    pub kind: String,
    /// Parent version.
    pub parent_version_id: Option<Uuid>,
    /// Author.
    pub created_by: Option<Uuid>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Operation sequence the version corresponds to.
    pub op_seq: i64,
}

/// Comment representation.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CommentView {
    /// Comment id.
    pub id: Uuid,
    /// File id.
    pub file_id: Uuid,
    /// Page id (UUID from the document format).
    pub page_id: Uuid,
    /// Optional node id.
    pub node_id: Option<Uuid>,
    /// Author user id.
    pub author_id: Uuid,
    /// Author display name.
    pub author_name: String,
    /// Comment body.
    pub body: String,
    /// Parent comment for replies.
    pub parent_id: Option<Uuid>,
    /// Resolution timestamp.
    pub resolved_at: Option<DateTime<Utc>>,
    /// User that resolved the comment.
    pub resolved_by: Option<Uuid>,
    /// Canvas anchor X.
    pub anchor_x: Option<f64>,
    /// Canvas anchor Y.
    pub anchor_y: Option<f64>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Asset metadata.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AssetView {
    /// Asset id.
    pub id: Uuid,
    /// File the asset was first uploaded to.
    pub file_id: Uuid,
    /// Asset kind (`image` or `font`).
    pub kind: String,
    /// Lowercase hex sha256.
    pub sha256: String,
    /// Detected MIME type.
    pub mime: String,
    /// Size in bytes.
    pub byte_size: i64,
    /// Original client file name (sanitized, informational only).
    pub original_name: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Share link metadata (the raw token is only ever returned on creation).
#[derive(Debug, Clone, Serialize)]
pub struct ShareView {
    /// Share id.
    pub id: Uuid,
    /// Shared file.
    pub file_id: Uuid,
    /// `view`, `comment` or `edit`.
    pub permission: String,
    /// Whether the share is password protected.
    pub has_password: bool,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// Creating user.
    pub created_by: Option<Uuid>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Revocation timestamp.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Audit event.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditEventView {
    /// Event id.
    pub id: i64,
    /// Acting user.
    pub actor_id: Option<Uuid>,
    /// Action name.
    pub action: String,
    /// Target type.
    pub target_type: String,
    /// Target id.
    pub target_id: Option<String>,
    /// Structured metadata.
    pub metadata: serde_json::Value,
    /// Client IP.
    pub ip: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Offset-paginated envelope.
#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
    /// Items on this page.
    pub items: Vec<T>,
    /// 1-based page number.
    pub page: u32,
    /// Effective page size.
    pub limit: u32,
    /// Total number of matching rows.
    pub total: i64,
    /// Total number of pages (at least 1).
    pub total_pages: u32,
}

impl<T> Page<T> {
    /// Build a page envelope.
    pub fn new(items: Vec<T>, page: crate::pagination::PageRequest, total: i64) -> Self {
        Self {
            items,
            page: page.page,
            limit: page.limit,
            total,
            total_pages: crate::pagination::PageRequest::pages_for(total, page.limit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::PageRequest;

    #[test]
    fn page_counts_are_computed() {
        let request = PageRequest::from_optional(Some(2), Some(10));
        let page = Page::new(vec![1_u32, 2], request, 25);
        assert_eq!(page.page, 2);
        assert_eq!(page.limit, 10);
        assert_eq!(page.total, 25);
        assert_eq!(page.total_pages, 3);
    }

    #[test]
    fn user_view_does_not_leak_password() {
        let view = UserView {
            id: Uuid::nil(),
            email: "ada@example.com".to_owned(),
            display_name: "Ada".to_owned(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(json.contains("ada@example.com"));
        assert!(!json.contains("password"));
    }
}
