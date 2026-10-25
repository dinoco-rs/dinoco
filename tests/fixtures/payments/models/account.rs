#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "account")]
pub struct Account {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub email: String,

    pub name: String,

    pub document: Option<String>,

    pub phone: Option<String>,

    #[dinoco(default = true)]
    pub active: bool,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(updated_at, default = ::dinoco::chrono::Utc::now())]
    pub updated_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    pub businesses: Vec<Business>,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    pub supports: Vec<Support>,

    #[dinoco(one_to_many, foreign_key = "account_id", references = "id")]
    pub support_participants: Vec<SupportParticipant>,

}
