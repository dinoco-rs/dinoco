use tokio_postgres::{Client, NoTls};

pub async fn connect() -> Client {
    let (client, connection) = tokio_postgres::connect("host=localhost user=postgres password=postgres dbname=postgres", NoTls)
        .await
        .expect("Erro ao conectar");

    // spawn da conexão (obrigatório)
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Erro na conexão: {}", e);
        }
    });

    client
}

pub struct PostgresIntrospector {
    pub client: Client,
}

impl PostgresIntrospector {
    pub async fn introspect(&self) -> Result<DatabaseSchema, tokio_postgres::Error> {
        let tables = self.get_tables().await?;

        let mut result = Vec::new();

        for table in tables {
            let columns = self.get_columns(&table).await?;
            let primary_keys = self.get_primary_keys(&table).await?;
            let foreign_keys = self.get_foreign_keys(&table).await?;

            result.push(Table {
                name: table,
                columns,
                primary_keys,
                foreign_keys,
            });
        }

        Ok(DatabaseSchema { tables: result })
    }

    async fn get_tables(&self) -> Result<Vec<String>, tokio_postgres::Error> {
        let rows = self
            .client
            .query(
                "
                SELECT table_name
                FROM information_schema.tables
                WHERE table_schema = 'public'
                ",
                &[],
            )
            .await?;

        Ok(rows.into_iter().map(|row| row.get::<_, String>("table_name")).collect())
    }

    async fn get_columns(&self, table: &str) -> Result<Vec<Column>, tokio_postgres::Error> {
        let rows = self
            .client
            .query(
                "
                SELECT column_name, data_type, is_nullable, column_default
                FROM information_schema.columns
                WHERE table_name = $1
                ",
                &[&table],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                println!("{:?}", row);

                Column {
                    name: row.get("column_name"),
                    db_type: row.get("data_type"),
                    nullable: row.get::<_, String>("is_nullable") == "YES",
                    default: row.get("column_default"),
                }
            })
            .collect())
    }

    async fn get_primary_keys(&self, table: &str) -> Result<Vec<String>, tokio_postgres::Error> {
        let rows = self
            .client
            .query(
                "
                SELECT kcu.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                WHERE tc.constraint_type = 'PRIMARY KEY'
                  AND tc.table_name = $1
                ",
                &[&table],
            )
            .await?;

        Ok(rows.into_iter().map(|row| row.get::<_, String>("column_name")).collect())
    }

    async fn get_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKey>, tokio_postgres::Error> {
        let rows = self
            .client
            .query(
                "
                SELECT
                    kcu.column_name,
                    ccu.table_name AS foreign_table,
                    ccu.column_name AS foreign_column
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                  ON tc.constraint_name = kcu.constraint_name
                JOIN information_schema.constraint_column_usage ccu
                  ON ccu.constraint_name = tc.constraint_name
                WHERE tc.constraint_type = 'FOREIGN KEY'
                  AND tc.table_name = $1
                ",
                &[&table],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| ForeignKey {
                column: row.get("column_name"),
                references_table: row.get("foreign_table"),
                references_column: row.get("foreign_column"),
            })
            .collect())
    }
}

pub async fn database_import_command() {
    let client = connect().await;

    let introspector = PostgresIntrospector { client };
    let schema = introspector.introspect().await.unwrap();

    println!("{:#?}", schema);
}

#[derive(Debug)]
pub struct DatabaseSchema {
    pub tables: Vec<Table>,
}

#[derive(Debug)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub primary_keys: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(Debug)]
pub struct Column {
    pub name: String,
    pub db_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

#[derive(Debug)]
pub struct ForeignKey {
    pub column: String,
    pub references_table: String,
    pub references_column: String,
}
