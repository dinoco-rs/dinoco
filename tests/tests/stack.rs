//! Runs typical flows on a thread with a reduced stack, the way a small worker
//! thread or a deep call chain leaves little room for a future polled on it.
//!
//! A debug build is where this bites: futures are moved around unoptimized,
//! each move a stack copy. A stack overflow aborts the whole test binary, so
//! these flows live in their own file.

#[path = "../fixtures/payments/models/mod.rs"]
#[allow(dead_code, unused_imports, clippy::upper_case_acronyms)]
mod models;

use std::future::Future;

use dinoco::{DinocoClient, TestAmbient, find_first, insert_into, transaction, update};
use models::*;

const STACK_SIZE: usize = 256 * 1024;

const SCHEMA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/payments/schema.dinoco");

/// Runs `flow` to completion on a `current_thread` runtime, on a thread whose
/// stack is [`STACK_SIZE`] bytes.
fn on_small_stack<F, Fut>(flow: F) -> anyhow::Result<()>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>>,
{
    std::thread::Builder::new()
        .name("small-stack".to_string())
        .stack_size(STACK_SIZE)
        .spawn(move || tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(flow()))?
        .join()
        .expect("the flow panicked")
}

#[test]
fn nested_includes_and_a_transaction_fit_a_small_stack() -> anyhow::Result<()> {
    on_small_stack(|| async {
        let client = TestAmbient::new().schema(SCHEMA).create().await?;
        let message_id = seed(&client).await?;

        let message = read_message(&client, &message_id).await?;
        let support = message.support.as_ref().expect("support");
        let pix = support.pix.as_ref().expect("pix");
        assert_eq!(pix.business.as_ref().and_then(|business| business.account.as_ref()).expect("account").name, "Ada");
        assert_eq!(
            message.participant.as_ref().and_then(|participant| participant.account.as_ref()).expect("author").name,
            "Ada"
        );

        let read = transaction(&client, |tx| async move {
            let message = find_first::<SupportMessage>()
                .where_(|row| row.id.eq(&message_id))
                .includes(|row| row.support().includes(|support| support.pix()))
                .execute(tx)
                .await?
                .expect("message");
            update::<Pix>()
                .where_(|row| {
                    row.id.eq(&message.support.as_ref().and_then(|support| support.pix.as_ref()).expect("pix").id)
                })
                .update(|row| row.status.set(ChargeStatus::PAID))
                .execute(tx)
                .await?;
            update::<SupportMessage>()
                .where_(|row| row.id.eq(&message.id))
                .update(|row| row.read_at.set(Some(dinoco::chrono::Utc::now())))
                .execute(tx)
                .await?;

            Ok(message)
        })
        .await?;
        assert_eq!(read.support.and_then(|support| support.pix).expect("pix").status, ChargeStatus::PENDING);

        let paid = find_first::<Pix>().where_(|row| row.status.eq(ChargeStatus::PAID)).execute(&client).await?;
        assert!(paid.is_some(), "the transaction committed");

        Ok(())
    })
}

/// A message with its author and its support, the support's charge, and the
/// charge's business and account.
async fn read_message(client: &DinocoClient, id: &str) -> anyhow::Result<SupportMessage> {
    let message = find_first::<SupportMessage>()
        .where_(|message| message.id.eq(id))
        .includes(|message| message.participant().includes(|participant| participant.account()))
        .includes(|message| {
            message.support().includes(|support| {
                support.pix().includes(|pix| pix.business().includes(|business| business.account()))
            })
        })
        .execute(client)
        .await?;

    Ok(message.expect("message"))
}

/// One account with a business, a paid-out charge and a support conversation
/// about it; returns the message's id.
async fn seed(client: &DinocoClient) -> anyhow::Result<String> {
    let account = Account::new("ada@dinoco.rs".to_string(), "Ada".to_string());
    insert_into::<Account>().values(&account).execute(client).await?;

    let business = Business::new(account.id.clone(), "Ada Pagamentos LTDA".to_string(), "00000000000100".to_string());
    insert_into::<Business>().values(&business).execute(client).await?;

    let mut profit = Profit::new(business.id.clone(), 990, 10);
    profit.description = Some("Pix fee".to_string());
    insert_into::<Profit>().values(&profit).execute(client).await?;

    let job = BackgroundJob::new("charges".to_string());
    insert_into::<BackgroundJob>().values(&job).execute(client).await?;

    let mut pix = Pix::new(
        business.id.clone(),
        1_000,
        "txid-1".to_string(),
        "00020126".to_string(),
        dinoco::chrono::Utc::now() + dinoco::chrono::Duration::hours(1),
    );
    pix.profit_id = Some(profit.id.clone());
    pix.background_job_id = Some(job.id.clone());
    insert_into::<Pix>().values(&pix).execute(client).await?;

    let mut support = Support::new("2026-0001".to_string(), "Pix not credited".to_string());
    support.business_id = Some(business.id.clone());
    support.account_id = Some(account.id.clone());
    support.pix_id = Some(pix.id.clone());
    insert_into::<Support>().values(&support).execute(client).await?;

    let mut participant = SupportParticipant::new(support.id.clone(), ParticipantRole::CUSTOMER);
    participant.account_id = Some(account.id.clone());
    insert_into::<SupportParticipant>().values(&participant).execute(client).await?;

    let mut message = SupportMessage::new(support.id.clone(), "The Pix was paid an hour ago.".to_string());
    message.participant_id = Some(participant.id.clone());
    insert_into::<SupportMessage>().values(&message).execute(client).await?;

    Ok(message.id)
}
