use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Uuid,
    pub github_id: i64,
    pub handle: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "member_role", rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Manager,
    Editor,
    Viewer,
}

impl MemberRole {
    pub fn can_push(self) -> bool {
        matches!(self, Self::Owner | Self::Manager | Self::Editor)
    }

    pub fn can_merge(self) -> bool {
        matches!(self, Self::Owner | Self::Manager)
    }

    pub fn can_read(self) -> bool {
        true
    }
}
