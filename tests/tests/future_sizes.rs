//! Size budgets for the future of every public builder and for the models
//! generated from `fixtures/payments/schema.dinoco`.
//!
//! An `async fn` keeps every value it holds across an `.await` inside its
//! future, and the caller's future embeds that one in turn, so a few bytes in
//! a builder or a model end up in every handler that awaits it. The futures
//! here are only created and measured, never polled.
//!
//! A size over its budget fails the test. Raise a budget only with the
//! measured reason in the commit message. Debug and release builds measure
//! the same. To see where the bytes of a future go:
//! `RUSTC_BOOTSTRAP=1 cargo rustc -p dinoco_tests --test future_sizes --profile test -- -Zprint-type-sizes`.

#[path = "../fixtures/payments/models/mod.rs"]
#[allow(dead_code, unused_imports, clippy::upper_case_acronyms)]
mod models;

use std::future::Future;
use std::mem::{size_of, size_of_val};

use dinoco::{
    Backend, DinocoClient, SqliteAdapter, TransactionContext, TransactionError, count, delete, delete_many, exists,
    find_and_update, find_first, find_many, insert_into, insert_many, transaction, transaction_with_error, update,
    update_many,
};
use models::*;

// Budgets in bytes: the size measured when they were set, plus about 5%.

// Models: a singular relation is an `Option<Box<T>>` of 8 bytes, so a model is
// as large as its own columns, however many models it points to.
const ACCOUNT: usize = 240;
const BUSINESS: usize = 336;
const BUSINESS_SETTINGS: usize = 120;
const ANONYMOUS: usize = 168;
const PROFIT: usize = 200;
const BACKGROUND_JOB: usize = 232;
const PIX: usize = 304;
const TICKET: usize = 296;
const PAYMENT_LINK: usize = 296;
const SUPPORT: usize = 368;
const SUPPORT_PARTICIPANT: usize = 208;
const SUPPORT_MESSAGE: usize = 168;

// Reads. Includes are boxed loaders, so they don't change a read's size.
const FIND_FIRST: usize = 416;
const FIND_FIRST_TX: usize = 408;
const FIND_FIRST_NESTED_INCLUDES: usize = 416;
const FIND_FIRST_NESTED_INCLUDES_TX: usize = 408;
const FIND_FIRST_NESTED_INCLUDES_TRANSFORM: usize = 424;
const FIND_MANY: usize = 416;
const FIND_MANY_TX: usize = 408;
const FIND_MANY_NESTED_INCLUDES: usize = 416;
const FIND_MANY_NESTED_INCLUDES_TX: usize = 408;
const FIND_MANY_TRANSFORM: usize = 424;
// A counting find is a find plus its `M::Count`: the count runs to completion
// before the find starts, and relation counts are awaited boxed.
const FIND_MANY_COUNT: usize = 440;
const FIND_MANY_COUNT_TX: usize = 432;
const FIND_MANY_COUNT_NESTED_INCLUDES: usize = 440;
const FIND_MANY_COUNT_RELATION_COUNTS: usize = 440;
const FIND_MANY_COUNT_RELATION_COUNTS_TX: usize = 432;
const FIND_MANY_COUNT_TRANSFORM: usize = 448;
const COUNT: usize = 264;
const COUNT_TX: usize = 256;
const COUNT_RELATION_COUNTS: usize = 264;
const COUNT_RELATION_COUNTS_TX: usize = 256;
const EXISTS: usize = 80;
const EXISTS_TX: usize = 72;
const AGGREGATE: usize = 384;
const AGGREGATE_TX: usize = 376;
const AGGREGATE_GROUP_BY: usize = 392;
const AGGREGATE_GROUP_BY_TX: usize = 384;

// Writes.
const INSERT_INTO: usize = 208;
const INSERT_INTO_TX: usize = 200;
const INSERT_INTO_OWNED: usize = 496;
const INSERT_INTO_RETURNING: usize = 248;
const INSERT_INTO_RETURNING_TX: usize = 240;
const INSERT_INTO_RETURNING_TRANSFORM: usize = 256;
const INSERT_MANY: usize = 224;
const INSERT_MANY_TX: usize = 216;
const INSERT_MANY_OWNED_RETURNING: usize = 264;
const UPDATE: usize = 112;
const UPDATE_TX: usize = 104;
const UPDATE_RETURNING: usize = 240;
const UPDATE_RETURNING_TX: usize = 232;
const UPDATE_RETURNING_TRANSFORM: usize = 248;
const UPDATE_MANY: usize = 112;
const UPDATE_MANY_TX: usize = 104;
const DELETE: usize = 72;
const DELETE_TX: usize = 56;
const DELETE_RETURNING: usize = 72;
const DELETE_RETURNING_TRANSFORM: usize = 80;
const DELETE_MANY: usize = 72;
const DELETE_MANY_TX: usize = 56;
const FIND_AND_UPDATE: usize = 192;
const FIND_AND_UPDATE_TX: usize = 184;
const FIND_AND_UPDATE_WITH_RELATIONS: usize = 192;

// Transactions: the closure's future, plus the transaction's own state.
const TRANSACTION_READ_WRITE: usize = 576;
const TRANSACTION_WITH_ERROR_READ_WRITE: usize = 576;
const TRANSACTION_RETURNING_THREE_MODELS: usize = 1240;

// An application handler that keeps a model across awaits.
const HANDLER: usize = 616;

struct Sizes {
    rows: Vec<(&'static str, usize, usize)>,
}

impl Sizes {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn future<F: Future>(&mut self, name: &'static str, budget: usize, future: F) {
        self.rows.push((name, size_of_val(&future), budget));
    }

    fn model<M>(&mut self, name: &'static str, budget: usize) {
        self.rows.push((name, size_of::<M>(), budget));
    }

    /// Prints every size (visible with `--nocapture`) and fails with each one
    /// over its budget.
    fn check(self, title: &str) {
        println!("\n{title}\n| item | bytes | budget |\n|---|---|---|");
        for (name, size, budget) in &self.rows {
            println!("| {name} | {size} | {budget} |");
        }

        let over = self
            .rows
            .iter()
            .filter(|(_, size, budget)| size > budget)
            .map(|(name, size, budget)| format!("{name}: {size} bytes > budget {budget}"))
            .collect::<Vec<_>>();
        assert!(over.is_empty(), "{}", over.join("\n"));
    }
}

async fn client() -> DinocoClient {
    DinocoClient::new(Backend::Sqlite(SqliteAdapter::memory().await.expect("in-memory sqlite")))
}

fn pix(txid: &str) -> Pix {
    Pix::new("business".to_string(), 1_000, txid.to_string(), "qr".to_string(), dinoco::chrono::Utc::now())
}

#[test]
fn generated_models_stay_within_budget() {
    let mut sizes = Sizes::new();
    sizes.model::<Account>("Account", ACCOUNT);
    sizes.model::<Business>("Business", BUSINESS);
    sizes.model::<BusinessSettings>("BusinessSettings", BUSINESS_SETTINGS);
    sizes.model::<Anonymous>("Anonymous", ANONYMOUS);
    sizes.model::<Profit>("Profit", PROFIT);
    sizes.model::<BackgroundJob>("BackgroundJob", BACKGROUND_JOB);
    sizes.model::<Pix>("Pix", PIX);
    sizes.model::<Ticket>("Ticket", TICKET);
    sizes.model::<PaymentLink>("PaymentLink", PAYMENT_LINK);
    sizes.model::<Support>("Support", SUPPORT);
    sizes.model::<SupportParticipant>("SupportParticipant", SUPPORT_PARTICIPANT);
    sizes.model::<SupportMessage>("SupportMessage", SUPPORT_MESSAGE);
    sizes.check("Generated models");
}

#[tokio::test]
async fn read_futures_stay_within_budget() {
    let client = client().await;
    let client = &client;
    let tx = TransactionContext;
    let mut sizes = Sizes::new();

    sizes.future("find_first", FIND_FIRST, find_first::<SupportMessage>().execute(client));
    sizes.future("find_first (tx)", FIND_FIRST_TX, find_first::<SupportMessage>().execute(tx));
    sizes.future("find_first + 3 nested includes", FIND_FIRST_NESTED_INCLUDES, nested_find_first().execute(client));
    sizes.future("find_first + 3 nested includes (tx)", FIND_FIRST_NESTED_INCLUDES_TX, nested_find_first().execute(tx));
    sizes.future(
        "find_first + 3 nested includes + transform",
        FIND_FIRST_NESTED_INCLUDES_TRANSFORM,
        nested_find_first().transform(|message| message.participant).execute(client),
    );

    sizes.future("find_many", FIND_MANY, find_many::<Pix>().execute(client));
    sizes.future("find_many (tx)", FIND_MANY_TX, find_many::<Pix>().execute(tx));
    sizes.future("find_many + nested includes", FIND_MANY_NESTED_INCLUDES, nested_find_many().execute(client));
    sizes.future("find_many + nested includes (tx)", FIND_MANY_NESTED_INCLUDES_TX, nested_find_many().execute(tx));
    sizes.future(
        "find_many + transform",
        FIND_MANY_TRANSFORM,
        find_many::<Pix>().transform(|pix| pix.id).execute(client),
    );
    sizes.future("find_many + count", FIND_MANY_COUNT, find_many::<Pix>().count().execute(client));
    sizes.future("find_many + count (tx)", FIND_MANY_COUNT_TX, find_many::<Pix>().count().execute(tx));
    sizes.future(
        "find_many + nested includes + count",
        FIND_MANY_COUNT_NESTED_INCLUDES,
        nested_find_many().count().execute(client),
    );
    sizes.future(
        "find_many + count with relation counts",
        FIND_MANY_COUNT_RELATION_COUNTS,
        counted_pixes().execute(client),
    );
    sizes.future(
        "find_many + count with relation counts (tx)",
        FIND_MANY_COUNT_RELATION_COUNTS_TX,
        counted_pixes().execute(tx),
    );
    sizes.future(
        "find_many + count + transform",
        FIND_MANY_COUNT_TRANSFORM,
        find_many::<Pix>().count().transform(|pix| pix.id).execute(client),
    );

    sizes.future("count", COUNT, count::<Support>().execute(client));
    sizes.future("count (tx)", COUNT_TX, count::<Support>().execute(tx));
    sizes.future("count + relation counts", COUNT_RELATION_COUNTS, counted_supports().execute(client));
    sizes.future("count + relation counts (tx)", COUNT_RELATION_COUNTS_TX, counted_supports().execute(tx));

    sizes.future("exists", EXISTS, exists::<Support>().execute(client));
    sizes.future("exists (tx)", EXISTS_TX, exists::<Support>().execute(tx));

    sizes.future("aggregate", AGGREGATE, find_many::<Pix>().pluck(|pix| pix.amount_cents.sum()).execute(client));
    sizes.future("aggregate (tx)", AGGREGATE_TX, find_many::<Pix>().pluck(|pix| pix.amount_cents.sum()).execute(tx));
    sizes.future(
        "aggregate + group_by",
        AGGREGATE_GROUP_BY,
        find_many::<Pix>().pluck(|pix| pix.amount_cents.sum()).group_by(|pix| pix.status).execute(client),
    );
    sizes.future(
        "aggregate + group_by (tx)",
        AGGREGATE_GROUP_BY_TX,
        find_many::<Pix>().pluck(|pix| pix.amount_cents.sum()).group_by(|pix| pix.status).execute(tx),
    );

    sizes.check("Read futures");
}

#[tokio::test]
async fn write_futures_stay_within_budget() {
    let client = client().await;
    let client = &client;
    let tx = TransactionContext;
    let item = pix("pix-1");
    let mut sizes = Sizes::new();

    sizes.future("insert_into", INSERT_INTO, insert_into::<Pix>().values(&item).execute(client));
    sizes.future("insert_into (tx)", INSERT_INTO_TX, insert_into::<Pix>().values(&item).execute(tx));
    sizes.future("insert_into owned", INSERT_INTO_OWNED, insert_into::<Pix>().values(pix("pix-2")).execute(client));
    sizes.future(
        "insert_into + returning",
        INSERT_INTO_RETURNING,
        insert_into::<Pix>().values(&item).returning::<Pix>().execute(client),
    );
    sizes.future(
        "insert_into + returning (tx)",
        INSERT_INTO_RETURNING_TX,
        insert_into::<Pix>().values(&item).returning::<Pix>().execute(tx),
    );
    sizes.future(
        "insert_into + returning + transform",
        INSERT_INTO_RETURNING_TRANSFORM,
        insert_into::<Pix>().values(&item).returning::<Pix>().transform(|pix| pix.id).execute(client),
    );

    sizes.future("insert_many", INSERT_MANY, insert_many::<Pix>().values([&item]).execute(client));
    sizes.future("insert_many (tx)", INSERT_MANY_TX, insert_many::<Pix>().values([&item]).execute(tx));
    sizes.future(
        "insert_many owned + returning",
        INSERT_MANY_OWNED_RETURNING,
        insert_many::<Pix>().values(vec![pix("pix-2"), pix("pix-3")]).returning::<Pix>().execute(client),
    );

    sizes.future("update", UPDATE, paid_pix().execute(client));
    sizes.future("update (tx)", UPDATE_TX, paid_pix().execute(tx));
    sizes.future("update + returning", UPDATE_RETURNING, paid_pix().returning::<Pix>().execute(client));
    sizes.future("update + returning (tx)", UPDATE_RETURNING_TX, paid_pix().returning::<Pix>().execute(tx));
    sizes.future(
        "update + returning + transform",
        UPDATE_RETURNING_TRANSFORM,
        paid_pix().returning::<Pix>().transform(|pix| pix.id).execute(client),
    );

    sizes.future("update_many", UPDATE_MANY, expired_pixes().execute(client));
    sizes.future("update_many (tx)", UPDATE_MANY_TX, expired_pixes().execute(tx));

    sizes.future("delete", DELETE, delete::<Pix>().where_(|pix| pix.id.eq("pix-1")).execute(client));
    sizes.future("delete (tx)", DELETE_TX, delete::<Pix>().where_(|pix| pix.id.eq("pix-1")).execute(tx));
    sizes.future(
        "delete + returning",
        DELETE_RETURNING,
        delete::<Pix>().where_(|pix| pix.id.eq("pix-1")).returning::<Pix>().execute(client),
    );
    sizes.future(
        "delete + returning + transform",
        DELETE_RETURNING_TRANSFORM,
        delete::<Pix>().where_(|pix| pix.id.eq("pix-1")).returning::<Pix>().transform(|pix| pix.id).execute(client),
    );

    sizes.future("delete_many", DELETE_MANY, delete_many::<Pix>().where_(|pix| pix.paid_at.null()).execute(client));
    sizes.future("delete_many (tx)", DELETE_MANY_TX, delete_many::<Pix>().where_(|pix| pix.paid_at.null()).execute(tx));

    sizes.future("find_and_update", FIND_AND_UPDATE, claimed_job().execute(client));
    sizes.future("find_and_update (tx)", FIND_AND_UPDATE_TX, claimed_job().execute(tx));
    sizes.future(
        "find_and_update of a model with relations",
        FIND_AND_UPDATE_WITH_RELATIONS,
        find_and_update::<SupportMessage>()
            .where_(|message| message.id.eq("message-1"))
            .update(|message| message.read_at.set(Some(dinoco::chrono::Utc::now())))
            .execute(client),
    );

    sizes.check("Write futures");
}

#[tokio::test]
async fn transaction_futures_stay_within_budget() {
    let client = client().await;
    let client = &client;
    let mut sizes = Sizes::new();

    sizes.future(
        "transaction: read + write",
        TRANSACTION_READ_WRITE,
        transaction(client, |tx| async move {
            let message = nested_find_first().execute(tx).await?;
            paid_pix().execute(tx).await?;
            Ok(message)
        }),
    );
    sizes.future(
        "transaction_with_error: read + write",
        TRANSACTION_WITH_ERROR_READ_WRITE,
        transaction_with_error::<std::io::Error, _>(client, async move |tx| {
            let message = nested_find_first().execute(tx).await?;
            paid_pix().execute(tx).await?;
            if message.is_none() {
                return Err(TransactionError::Custom(std::io::ErrorKind::NotFound.into()));
            }
            Ok(message)
        }),
    );
    // The result lives once in the transaction's future while it commits.
    sizes.future(
        "transaction returning three models",
        TRANSACTION_RETURNING_THREE_MODELS,
        transaction(client, |tx| async move {
            let pix = find_first::<Pix>().execute(tx).await?;
            let support = find_first::<Support>().execute(tx).await?;
            let message = find_first::<SupportMessage>().execute(tx).await?;
            Ok((pix, support, message))
        }),
    );

    sizes.check("Transaction futures");
}

/// The shape of an application handler: it loads a message with its
/// relations, keeps it across further awaits, and moves a relation out of it.
#[tokio::test]
async fn handler_futures_stay_within_budget() {
    let client = client().await;
    let mut sizes = Sizes::new();

    sizes.future("handler holding a SupportMessage", HANDLER, reply_to_message(&client));

    sizes.check("Handler futures");
}

async fn reply_to_message(client: &DinocoClient) -> anyhow::Result<()> {
    let Some(message) = nested_find_first().execute(client).await? else {
        return Ok(());
    };
    let Some(participant) = message.participant else {
        return Ok(());
    };

    update::<SupportParticipant>()
        .where_(|row| row.id.eq(&participant.id))
        .update(|row| row.joined_at.set(dinoco::chrono::Utc::now()))
        .execute(client)
        .await?;
    update::<SupportMessage>()
        .where_(|row| row.id.eq(&message.id))
        .update(|row| row.read_at.set(Some(dinoco::chrono::Utc::now())))
        .execute(client)
        .await?;

    Ok(())
}

/// A message with its author and its support, the support's charge and the
/// charge's business and account: three levels below the support.
fn nested_find_first() -> dinoco::FindFirst<SupportMessage> {
    find_first::<SupportMessage>()
        .where_(|message| message.id.eq("message-1"))
        .includes(|message| message.participant().includes(|participant| participant.account()))
        .includes(|message| {
            message.support().includes(|support| {
                support.pix().includes(|pix| pix.business().includes(|business| business.account()))
            })
        })
}

fn nested_find_many() -> dinoco::FindMany<Pix> {
    find_many::<Pix>()
        .includes(|pix| pix.business().includes(|business| business.account()))
        .includes(|pix| pix.profit())
        .includes(|pix| pix.supports().includes(|support| support.messages()))
}

/// Pixes of one business with every pix of it counted, and its supports.
fn counted_pixes() -> dinoco::FindMany<Pix, Pix, dinoco::WithCount<impl dinoco::CountFilters<Pix>>> {
    find_many::<Pix>()
        .where_(|pix| pix.business_id.eq("business"))
        .where_(|pix| pix.status.eq(ChargeStatus::PAID))
        .take(20)
        .count_transform(|count| count.remove(|pix| pix.status).includes(|pix| pix.supports()))
}

fn counted_supports() -> dinoco::Count<Support> {
    count::<Support>()
        .where_(|support| support.status.eq(SupportStatus::OPEN))
        .includes(|support| support.messages())
        .includes(|support| support.participants())
}

fn paid_pix() -> dinoco::Update<Pix> {
    update::<Pix>().where_(|pix| pix.id.eq("pix-1")).update(|pix| pix.status.set(ChargeStatus::PAID))
}

fn expired_pixes() -> dinoco::UpdateMany<Pix> {
    update_many::<Pix>()
        .where_(|pix| pix.status.eq(ChargeStatus::PENDING))
        .update(|pix| pix.status.set(ChargeStatus::EXPIRED))
}

fn claimed_job() -> dinoco::FindAndUpdate<BackgroundJob> {
    find_and_update::<BackgroundJob>()
        .where_(|job| job.status.eq(JobStatus::QUEUED))
        .update(|job| job.status.set(JobStatus::RUNNING))
}
