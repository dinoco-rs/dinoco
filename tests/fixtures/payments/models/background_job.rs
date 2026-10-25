#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "background_job")]
pub struct BackgroundJob {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub queue: String,

    #[dinoco(enum, default = JobStatus::QUEUED)]
    pub status: JobStatus,

    #[dinoco(default = 0)]
    pub attempts: i64,

    pub payload: Option<::dinoco::serde_json::Value>,

    pub last_error: Option<String>,

    #[dinoco(default = ::dinoco::chrono::Utc::now())]
    pub run_at: ::dinoco::chrono::DateTime<::dinoco::chrono::Utc>,

    pub finished_at: Option<::dinoco::chrono::DateTime<::dinoco::chrono::Utc>>,

    #[dinoco(one_to_many, foreign_key = "background_job_id", references = "id")]
    pub pixes: Vec<Pix>,

    #[dinoco(one_to_many, foreign_key = "background_job_id", references = "id")]
    pub tickets: Vec<Ticket>,

    #[dinoco(one_to_many, foreign_key = "background_job_id", references = "id")]
    pub payment_links: Vec<PaymentLink>,

}
