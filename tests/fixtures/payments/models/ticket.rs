#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "ticket")]
pub struct Ticket {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub business_id: ::dinoco::Uuid,

    pub profit_id: Option<::dinoco::Uuid>,

    pub background_job_id: Option<::dinoco::Uuid>,

    pub amount_cents: i64,

    #[dinoco(enum, default = ChargeStatus::PENDING)]
    pub status: ChargeStatus,

    pub barcode: String,

    pub digitable_line: String,

    pub payer_name: String,

    pub payer_document: String,

    pub due_date: ::dinoco::chrono::NaiveDate,

    pub paid_at: Option<::dinoco::chrono::DateTime<::dinoco::chrono::Utc>>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub created_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    #[dinoco(many_to_one, foreign_key = "business_id", references = "id")]
    pub business: Option<Box<Business>>,

    #[dinoco(many_to_one, foreign_key = "profit_id", references = "id")]
    pub profit: Option<Box<Profit>>,

    #[dinoco(many_to_one, foreign_key = "background_job_id", references = "id")]
    pub background_job: Option<Box<BackgroundJob>>,

    #[dinoco(one_to_many, foreign_key = "ticket_id", references = "id")]
    pub supports: Vec<Support>,

}
