use std::env;
use std::fs::{self, read_to_string};
use std::path::Path;
use std::time::Duration;

use colored::*;
use indicatif::ProgressBar;

use dinoco_compiler::{ConnectionUrl, Database, ParsedConfig, ParsedSchema, compile, render_error};
use dinoco_engine::{DinocoAdapter, DinocoResult, MySqlAdapter, PostgresAdapter, SqliteAdapter};

use crate::helpers::encode_schema;
use crate::{create_migration_table, fetch, get_all_migrations, insert_migration};

pub async fn run_migrations() -> DinocoResult<()> {
    let schema_path = "dinoco/schema.dinoco";

    if !Path::new(schema_path).exists() {
        println!("\n{} {}\n", "✖".red().bold(), "Dinoco project not initialized.".bold());
        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), "Starting migrations run...".white());

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message("Compiling schema...");

    let source = match read_to_string(schema_path) {
        Ok(content) => content,
        Err(e) => {
            pb.finish_and_clear();
            println!("\n{} {}\n", "✖".red().bold(), "Failed to read schema file.".bold());
            println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            return Ok(());
        }
    };

    let parsed = match compile(&source) {
        Ok((_, parsed)) => {
            pb.suspend(|| {
                println!("{} {}", "✔".green().bold(), "Schema compiled successfully.".white());
            });
            parsed
        }
        Err(errs) => {
            pb.finish_and_clear();
            println!("\n{} {}\n", "✖".red().bold(), format!("Schema compilation failed ({} error(s)).", errs.len()).bold());

            for err in errs {
                println!("{}", render_error(&err, &source));
            }

            return Ok(());
        }
    };

    let (url, db_type) = {
        let ParsedConfig { database, database_url, .. } = &parsed.config;

        let url = match database_url {
            ConnectionUrl::Env(var_name) => match env::var(var_name) {
                Ok(val) => val,
                Err(_) => {
                    pb.finish_and_clear();
                    println!("\n{} {}\n", "✖".red().bold(), "Missing environment variable.".bold());
                    println!("  {} {}", "→ Variable:".yellow().bold(), var_name.cyan());
                    return Ok(());
                }
            },
            ConnectionUrl::Literal(url) => url.clone(),
        };

        (url, database.clone())
    };

    pb.set_message(format!("Connecting to {:?}...", db_type));

    match db_type {
        Database::Postgresql => match PostgresAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_run(adapter, &pb, parsed).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
        Database::Mysql => match MySqlAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_run(adapter, &pb, parsed).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
        Database::Sqlite => match SqliteAdapter::connect(url).await {
            Ok(adapter) => {
                pb.suspend(|| println!("{} {}", "✔".green().bold(), "Connected to database.".white()));
                execute_run(adapter, &pb, parsed).await?;
            }
            Err(e) => {
                pb.finish_and_clear();
                println!("\n{} {}\n", "✖".red().bold(), "Database connection failed.".bold());
                println!("  {} {}", "Reason:".yellow().bold(), e.to_string().white());
            }
        },
    }

    Ok(())
}

async fn execute_run<T>(adapter: T, pb: &ProgressBar, parsed_schema: ParsedSchema) -> DinocoResult<()>
where
    T: DinocoAdapter,
{
    pb.set_message("Verificando integridade do banco...");

    let tables = fetch(&adapter).await?;
    let has_history_table = tables.iter().any(|x| x.name == "_dinoco_migrations");

    if !has_history_table {
        pb.set_message("Inicializando tabela de histórico...");
        create_migration_table(&adapter).await?;
    }

    let applied_migrations = get_all_migrations(&adapter).await.unwrap_or_default();
    let applied_names: Vec<String> = applied_migrations.into_iter().map(|m| m.name).collect();

    let migrations_dir = Path::new("dinoco/migrations");
    if !migrations_dir.exists() {
        pb.finish_and_clear();
        println!("{} {}", "✔".green().bold(), "Nenhuma pasta de migrações encontrada.".white());
        return Ok(());
    }

    let mut local_folders = Vec::new();
    if let Ok(entries) = fs::read_dir(migrations_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    local_folders.push(name.to_string());
                }
            }
        }
    }

    local_folders.sort();

    let pending: Vec<String> = local_folders.into_iter().filter(|m| !applied_names.contains(m)).collect();

    pb.finish_and_clear();

    if pending.is_empty() {
        println!("{} {}", "✔".green().bold(), "O banco de dados já está atualizado.".white());
        return Ok(());
    }

    println!("{} {}", "✔".green().bold(), format!("Encontrada(s) {} migração(ões) pendente(s).", pending.len()).white());

    for migration_name in pending {
        println!("  {} Aplicando '{}'...", "→".cyan().bold(), migration_name);

        let sql_path = migrations_dir.join(&migration_name).join("migration.sql");

        let sql_content = fs::read_to_string(&sql_path)
            .map_err(|e| format!("Erro ao ler arquivo SQL em {}: {}", sql_path.display(), e))
            .unwrap();

        if !sql_content.trim().is_empty() {
            let statements = sql_content.split(';');

            for stmt in statements {
                let clean_stmt = stmt.trim();
                if clean_stmt.is_empty() {
                    continue;
                }

                if let Err(e) = adapter.execute(clean_stmt, &[]).await {
                    println!("    {} {} {}", "✖".red(), "Erro ao executar query na migração:".bold(), migration_name.yellow());
                    println!("    {} {}", "Query:".yellow(), clean_stmt.cyan());
                    println!("    {} {}", "Motivo:".red(), e);
                    return Ok(());
                }
            }
        }

        insert_migration(&adapter, &migration_name, encode_schema(parsed_schema.clone())).await?;
        println!("    {} Aplicada com sucesso.", "✔".green());
    }

    println!("\n{} {}", "✔".green().bold(), "Todas as migrações locais foram aplicadas!".white());

    Ok(())
}
