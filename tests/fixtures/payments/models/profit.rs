#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "profit")]
pub struct Profit {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub business_id: ::dinoco::Uuid,

    pub amount_cents: i64,

    pub fee_cents: i64,

    pub description: Option<String>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    pub business: Option<Box<Business>>,

    #[dinoco(one_to_many, foreign_key = "profit_id", references = "id")]
    pub pixes: Vec<Pix>,

    #[dinoco(one_to_many, foreign_key = "profit_id", references = "id")]
    pub tickets: Vec<Ticket>,

    #[dinoco(one_to_many, foreign_key = "profit_id", references = "id")]
    pub payment_links: Vec<PaymentLink>,

}
