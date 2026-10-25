#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, ::dinoco::DinocoEnum)]
#[serde(crate = "::dinoco::serde")]
pub enum ChargeStatus {
    #[default]
    #[dinoco(value = "PENDING")]
    #[serde(rename = "PENDING")]
    PENDING,
    #[dinoco(value = "PAID")]
    #[serde(rename = "PAID")]
    PAID,
    #[dinoco(value = "EXPIRED")]
    #[serde(rename = "EXPIRED")]
    EXPIRED,
    #[dinoco(value = "REFUNDED")]
    #[serde(rename = "REFUNDED")]
    REFUNDED,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, ::dinoco::DinocoEnum)]
#[serde(crate = "::dinoco::serde")]
pub enum JobStatus {
    #[default]
    #[dinoco(value = "QUEUED")]
    #[serde(rename = "QUEUED")]
    QUEUED,
    #[dinoco(value = "RUNNING")]
    #[serde(rename = "RUNNING")]
    RUNNING,
    #[dinoco(value = "DONE")]
    #[serde(rename = "DONE")]
    DONE,
    #[dinoco(value = "FAILED")]
    #[serde(rename = "FAILED")]
    FAILED,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, ::dinoco::DinocoEnum)]
#[serde(crate = "::dinoco::serde")]
pub enum SupportStatus {
    #[default]
    #[dinoco(value = "OPEN")]
    #[serde(rename = "OPEN")]
    OPEN,
    #[dinoco(value = "WAITING")]
    #[serde(rename = "WAITING")]
    WAITING,
    #[dinoco(value = "CLOSED")]
    #[serde(rename = "CLOSED")]
    CLOSED,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::dinoco::serde::Serialize, ::dinoco::serde::Deserialize, ::dinoco::DinocoEnum)]
#[serde(crate = "::dinoco::serde")]
pub enum ParticipantRole {
    #[default]
    #[dinoco(value = "CUSTOMER")]
    #[serde(rename = "CUSTOMER")]
    CUSTOMER,
    #[dinoco(value = "MERCHANT")]
    #[serde(rename = "MERCHANT")]
    MERCHANT,
    #[dinoco(value = "AGENT")]
    #[serde(rename = "AGENT")]
    AGENT,
}

mod account;
pub use account::*;
mod business;
pub use business::*;
mod business_settings;
pub use business_settings::*;
mod anonymous;
pub use anonymous::*;
mod profit;
pub use profit::*;
mod background_job;
pub use background_job::*;
mod pix;
pub use pix::*;
mod ticket;
pub use ticket::*;
mod payment_link;
pub use payment_link::*;
mod support;
pub use support::*;
mod support_participant;
pub use support_participant::*;
mod support_message;
pub use support_message::*;
