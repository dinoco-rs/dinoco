#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "support")]
pub struct Support {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub protocol: String,

    pub subject: String,

    #[dinoco(enum, default = SupportStatus::OPEN)]
    pub status: SupportStatus,

    pub business_id: Option<::dinoco::Uuid>,

    pub account_id: Option<::dinoco::Uuid>,

    pub anonymous_id: Option<::dinoco::Uuid>,

    pub pix_id: Option<::dinoco::Uuid>,

    pub ticket_id: Option<::dinoco::Uuid>,

    pub payment_link_id: Option<::dinoco::Uuid>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    pub closed_at: Option<::dinoco::chrono::DateTime<::dinoco::chrono::Utc>>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    pub business: Option<Box<Business>>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    pub account: Option<Box<Account>>,

    #[dinoco(many_to_one, foreign_key = "anonymous_id", references = "id")]
    pub anonymous: Option<Box<Anonymous>>,

    #[dinoco(many_to_one, foreign_key = "pix_id", references = "id")]
    pub pix: Option<Box<Pix>>,

    #[dinoco(many_to_one, foreign_key = "ticket_id", references = "id")]
    pub ticket: Option<Box<Ticket>>,

    #[dinoco(many_to_one, foreign_key = "payment_link_id", references = "id")]
    pub payment_link: Option<Box<PaymentLink>>,

    #[dinoco(one_to_many, foreign_key = "support_id", references = "id")]
    pub participants: Vec<SupportParticipant>,

    #[dinoco(one_to_many, foreign_key = "support_id", references = "id")]
    pub messages: Vec<SupportMessage>,

}
