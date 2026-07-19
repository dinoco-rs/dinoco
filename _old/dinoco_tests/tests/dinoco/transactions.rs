use dinoco::{
    DinocoAdapter, DinocoClient, DinocoResult, DinocoTransactionAdapter, DinocoValue, Expression, InsertModel, Model,
    Projection, Rowable, ScalarField, UpdateField, UpdateModel, count, find_first, find_many, insert_into, insert_many,
    transactions, tx, update,
};
use dinoco_engine::{MySqlAdapter, PostgresAdapter, SqliteAdapter};

mod common;

const TABLE_NAME: &str = "dinoco_core_transactions";

#[derive(Debug, Clone, Rowable)]
struct TxUser {
    id: i64,
    email: String,
    name: String,
}

struct TxUserWhere {
    id: ScalarField<i64>,
    email: ScalarField<String>,
    name: ScalarField<String>,
}

struct TxUserInclude {}

struct TxUserUpdate {
    email: UpdateField<String>,
    name: UpdateField<String>,
}

impl Model for TxUser {
    type Include = TxUserInclude;
    type Where = TxUserWhere;

    fn table_name() -> &'static str {
        TABLE_NAME
    }
}

impl Projection<TxUser> for TxUser {
    fn columns() -> &'static [&'static str] {
        &["id", "email", "name"]
    }
}

impl InsertModel for TxUser {
    fn insert_columns() -> &'static [&'static str] {
        &["id", "email", "name"]
    }

    fn into_insert_row(self) -> Vec<DinocoValue> {
        vec![self.id.into(), self.email.into(), self.name.into()]
    }

    fn insert_identity_conditions(&self) -> Vec<Expression> {
        vec![Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl UpdateModel for TxUser {
    fn update_columns() -> &'static [&'static str] {
        &["email", "name"]
    }

    fn into_update_row(self) -> Vec<DinocoValue> {
        vec![self.email.into(), self.name.into()]
    }

    fn update_identity_conditions(&self) -> Vec<Expression> {
        vec![Expression::Column("id".to_string()).eq(self.id)]
    }
}

impl Default for TxUserWhere {
    fn default() -> Self {
        Self { id: ScalarField::new("id"), email: ScalarField::new("email"), name: ScalarField::new("name") }
    }
}

impl Default for TxUserInclude {
    fn default() -> Self {
        Self {}
    }
}

impl Default for TxUserUpdate {
    fn default() -> Self {
        Self { email: UpdateField::new("email"), name: UpdateField::new("name") }
    }
}

#[tokio::test]
async fn sqlite_transactions_are_atomic() -> DinocoResult<()> {
    let client = DinocoClient::<SqliteAdapter>::new(
        common::sqlite_url("transactions-adapters"),
        vec![],
        dinoco::DinocoClientConfig::default(),
    )
    .await?;

    client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;
    create_sqlite_table(&client).await?;
    exercise_transaction_flow(&client).await
}

#[tokio::test]
async fn postgres_transactions_are_atomic() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_postgres().await;
        let client =
            DinocoClient::<PostgresAdapter>::new(common::postgres_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;
        create_postgres_table(&client).await?;
        exercise_transaction_flow(&client).await?;
        client.primary().execute(&format!(r#"DROP TABLE IF EXISTS "{TABLE_NAME}""#), &[]).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping postgres transaction adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

#[tokio::test]
async fn mysql_transactions_are_atomic() -> DinocoResult<()> {
    if let Err(err) = async {
        let _lock = common::lock_mysql().await;
        let client =
            DinocoClient::<MySqlAdapter>::new(common::mysql_url(), vec![], dinoco::DinocoClientConfig::default())
                .await?;

        client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await?;
        create_mysql_table(&client).await?;
        exercise_transaction_flow(&client).await?;
        client.primary().execute(&format!("DROP TABLE IF EXISTS `{TABLE_NAME}`"), &[]).await?;

        Ok(())
    }
    .await
    {
        if common::should_skip_external_adapter_test(&err) {
            eprintln!("skipping mysql transaction adapter test: {err}");
            return Ok(());
        }

        return Err(err);
    }

    Ok(())
}

async fn create_sqlite_table(client: &DinocoClient<SqliteAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                r#"CREATE TABLE "{TABLE_NAME}" (
                    "id" INTEGER PRIMARY KEY,
                    "email" TEXT NOT NULL UNIQUE,
                    "name" TEXT NOT NULL
                )"#
            ),
            &[],
        )
        .await
}

async fn create_postgres_table(client: &DinocoClient<PostgresAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                r#"CREATE TABLE "{TABLE_NAME}" (
                    "id" BIGINT PRIMARY KEY,
                    "email" TEXT NOT NULL UNIQUE,
                    "name" TEXT NOT NULL
                )"#
            ),
            &[],
        )
        .await
}

async fn create_mysql_table(client: &DinocoClient<MySqlAdapter>) -> DinocoResult<()> {
    client
        .primary()
        .execute(
            &format!(
                "CREATE TABLE `{TABLE_NAME}` (\
                    `id` BIGINT PRIMARY KEY,\
                    `email` VARCHAR(255) NOT NULL UNIQUE,\
                    `name` VARCHAR(255) NOT NULL\
                )"
            ),
            &[],
        )
        .await
}

async fn exercise_transaction_flow<A>(client: &DinocoClient<A>) -> DinocoResult<()>
where
    A: DinocoAdapter + DinocoTransactionAdapter + Send + Sync + 'static,
{
    let first = TxUser { id: 1, email: "alice@dinoco.dev".to_string(), name: "Alice".to_string() };
    let second = TxUser { id: 2, email: "bruno@dinoco.dev".to_string(), name: "Bruno".to_string() };
    let updated = TxUser { id: 1, email: "alice@dinoco.dev".to_string(), name: "Alice Updated".to_string() };

    let mut success_actions = Vec::<dinoco::TransactionAction<A>>::new();
    success_actions.push(tx(find_first::<TxUser>().cond(|w| w.id.eq(1_i64))));
    success_actions.push(tx(insert_many::<TxUser>().values::<TxUser, _>(vec![&first, &second])));
    success_actions.push(tx(update::<TxUser>().cond(|w| w.id.eq(1_i64)).values::<TxUser, _>(&updated)));

    transactions(success_actions).execute(client).await?;

    let inserted_count = count::<TxUser>().execute(client).await?;
    assert_eq!(inserted_count, 2);

    let updated_user = find_first::<TxUser>()
        .cond(|w| w.id.eq(1_i64))
        .execute(client)
        .await?
        .expect("user must exist after successful transaction");
    assert_eq!(updated_user.name, "Alice Updated");

    let rollback_user = TxUser { id: 1, email: "alice@dinoco.dev".to_string(), name: "Should Rollback".to_string() };
    let inserted_before_error = TxUser { id: 3, email: "caio@dinoco.dev".to_string(), name: "Caio".to_string() };
    let duplicate_email = TxUser { id: 4, email: "alice@dinoco.dev".to_string(), name: "Duplicated".to_string() };

    let mut rollback_actions = Vec::<dinoco::TransactionAction<A>>::new();
    rollback_actions.push(tx(update::<TxUser>().cond(|w| w.id.eq(1_i64)).values::<TxUser, _>(&rollback_user)));
    rollback_actions.push(tx(insert_into::<TxUser>().values::<TxUser, _>(&inserted_before_error)));
    rollback_actions.push(tx(insert_into::<TxUser>().values::<TxUser, _>(&duplicate_email)));

    let rollback_result = transactions(rollback_actions).execute(client).await;
    assert!(rollback_result.is_err(), "transaction should rollback on unique violation");

    let user_after_rollback = find_first::<TxUser>()
        .cond(|w| w.id.eq(1_i64))
        .execute(client)
        .await?
        .expect("user must still exist after rollback");
    assert_eq!(user_after_rollback.name, "Alice Updated");

    let ids = find_many::<TxUser>()
        .order_by(|w| w.id.asc())
        .execute(client)
        .await?
        .into_iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![1, 2], "failed transaction must not persist partial writes");

    Ok(())
}
