#[allow(unused_imports)]
use super::*;
use dinoco::Entity;

#[derive(Debug, Clone, Entity, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize)]
#[serde(crate = "::dinoco::serde")]
#[dinoco(table_name = "business_settings")]
pub struct BusinessSettings {
    #[dinoco(primary_key, auto_generate = uuid)]
    pub id: ::dinoco::Uuid,

    pub business_id: ::dinoco::Uuid,

    pub webhook_url: Option<String>,

    pub webhook_secret: Option<String>,

    #[dinoco(default = true)]
    pub notify_by_email: bool,

    #[dinoco(one_to_one, foreign_key = "business_id", references = "id")]
    pub business: Option<Box<Business>>,

}
