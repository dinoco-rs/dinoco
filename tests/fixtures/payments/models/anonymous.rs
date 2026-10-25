#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "anonymous")]
pub struct Anonymous {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub fingerprint: String,

    pub ip: Option<String>,

    pub user_agent: Option<String>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(one_to_many, foreign_key = "anonymous_id", references = "id")]
    pub supports: Vec<Support>,

    #[dinoco(one_to_many, foreign_key = "anonymous_id", references = "id")]
    pub support_participants: Vec<SupportParticipant>,

}
