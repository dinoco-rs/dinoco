use serde::{Deserialize, Serialize};
use serde_json::Value;

// ==========================================
// ENUMS
// ==========================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserRole {
    Admin,
    Member,
    Tester,
    Guest,
}

impl Default for UserRole {
    fn default() -> Self {
        Self::Member
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PostStatus {
    Draft,
    Published,
    Archived,
}

impl Default for PostStatus {
    fn default() -> Self {
        Self::Draft
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TicketPriority {
    Low,
    Medium,
    High,
}

impl Default for TicketPriority {
    fn default() -> Self {
        Self::Medium
    }
}

// ==========================================
// MODELS
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    // Campos escalares (Colunas)
    pub id: i32,
    pub name: String,
    pub age: Option<i32>,
    pub is_admin: bool,
    pub role: UserRole,
    pub created_at: String, // ou chrono::DateTime<chrono::Utc>
    pub updated_at: String,

    // Relacionamentos (Populados via queries com join/include)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Box<Profile>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub posts: Option<Vec<Post>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub liked_posts: Option<Vec<Post>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub followed_by: Option<Vec<User>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub following: Option<Vec<User>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub opened_tickets: Option<Vec<SupportTicket>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_tickets: Option<Vec<SupportTicket>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    // Campos escalares
    pub id: i32,
    pub bio: String,
    pub avatar: Option<String>,
    pub user_id: i32,

    // Relacionamentos
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<Box<User>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    // Campos escalares
    pub id: String, // UUID
    pub title: String,
    pub content: Option<String>,
    pub status: PostStatus,
    pub author_id: i32,
    pub created_at: String,

    // Relacionamentos
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Box<User>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub liked_by: Option<Vec<User>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<Category>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    // Campos escalares
    pub id: i32,
    pub name: String,

    // Relacionamentos
    #[serde(skip_serializing_if = "Option::is_none"1)]
    pub posts: Option<Vec<Post>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportTicket {
    // Campos escalares
    pub id: i32,
    pub title: String,
    pub priority: TicketPriority,
    pub creator_id: i32,
    pub assignee_id: Option<i32>,
    pub created_at: String,

    // Relacionamentos
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<Box<User>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Box<User>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLog {
    // Campos escalares
    pub id: i32,
    pub event: String,
    pub metadata: Option<Value>, // Mapeado para o serde_json::Value
    pub success: bool,
    pub timestamp: String,
}
