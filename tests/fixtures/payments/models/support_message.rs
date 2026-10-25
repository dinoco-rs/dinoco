#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "support_message")]
pub struct SupportMessage {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub support_id: ::dinoco::Uuid,

    pub participant_id: Option<::dinoco::Uuid>,

    pub body: String,

    pub attachment_url: Option<String>,

    pub read_at: Option<::dinoco::chrono::DateTime<::dinoco::chrono::Utc>>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(many_to_one, foreign_key = "support_id", references = "id")]
    pub support: Option<Box<Support>>,

    #[dinoco(many_to_one, foreign_key = "participant_id", references = "id")]
    pub participant: Option<Box<SupportParticipant>>,

}
