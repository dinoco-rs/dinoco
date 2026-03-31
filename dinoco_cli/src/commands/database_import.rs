use dinoco_engine::{DinocoAdapter, DinocoDatabaseRow, DinocoResult, DinocoRow, PostgresAdapter};
use futures::StreamExt;

pub async fn database_import_command() {
    let adapter = PostgresAdapter::connect("postgresql://postgres.votzhpldwahwltnonmnn:CriadorDeBot@aws-1-sa-east-1.pooler.supabase.com:5432/postgres".to_string())
        .await
        .unwrap();

    let mut stream = adapter
        .stream_as::<Table>("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'", &[])
        .await;

    while let Some(item) = stream.next().await {
        match item {
            Ok(user) => {
                println!("User: {:?}", user);
            }
            Err(e) => {
                eprintln!("Erro: {:?}", e);
            }
        }
    }
}

#[derive(Debug)]
pub struct DatabaseSchema {
    pub tables: Vec<Table>,
}

#[derive(Debug)]
pub struct Table {
    pub name: String,
    // pub columns: Vec<Column>,
    // pub primary_keys: Vec<String>,
    // pub foreign_keys: Vec<ForeignKey>,
}

impl DinocoRow for Table {
    fn from_row<R: DinocoDatabaseRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { name: row.get(0)? })
    }
}

// #[derive(Debug)]
// pub struct Column {
//     pub name: String,
//     pub db_type: String,
//     pub nullable: bool,
//     pub default: Option<String>,
// }

// #[derive(Debug)]
// pub struct ForeignKey {
//     pub column: String,
//     pub references_table: String,
//     pub references_column: String,
// }
