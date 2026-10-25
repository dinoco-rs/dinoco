#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "business")]
pub struct Business {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub account_id: ::dinoco::Uuid,

    pub legal_name: String,

    pub trade_name: Option<String>,

    pub document: String,

    #[dinoco(default = 0)]
    pub balance_cents: i64,

    #[dinoco(default = 0.0)]
    pub fee_percent: f64,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(updated_at, default = ::dinoco::chrono::Utc::now())]
    pub updated_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(many_to_one, foreign_key = "account_id", references = "id")]
    pub account: Option<Box<Account>>,

    #[dinoco(one_to_one, inverse, foreign_key = "business_id", references = "id")]
    pub settings: Option<Box<BusinessSettings>>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    pub profits: Vec<Profit>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    pub pixes: Vec<Pix>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    pub tickets: Vec<Ticket>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    pub payment_links: Vec<PaymentLink>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    pub supports: Vec<Support>,

    #[dinoco(one_to_many, foreign_key = "business_id", references = "id")]
    pub support_participants: Vec<SupportParticipant>,

}
