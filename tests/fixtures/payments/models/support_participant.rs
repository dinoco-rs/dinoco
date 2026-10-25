#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "support_participant")]
pub struct SupportParticipant {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub support_id: ::dinoco::Uuid,

    #[dinoco(enum)]
    pub role: ParticipantRole,

    pub account_id: Option<::dinoco::Uuid>,

    pub business_id: Option<::dinoco::Uuid>,

    pub anonymous_id: Option<::dinoco::Uuid>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub joined_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(many_to_one, foreign_key = "support_id", references = "id")]
    pub support: Option<Box<Support>>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    pub account: Option<Box<Account>>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    pub business: Option<Box<Business>>,

    #[dinoco(many_to_one, foreign_key = "anonymous_id", references = "id")]
    pub anonymous: Option<Box<Anonymous>>,

    #[dinoco(one_to_many, foreign_key = "participant_id", references = "id")]
    pub messages: Vec<SupportMessage>,

}
