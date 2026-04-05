use colored::*;

use dinoco_engine::DinocoResult;

pub async fn rollback_migration() -> DinocoResult<()> {
    println!(
        "{} {}",
        "⚠".yellow().bold(),
        "Rollback is temporarily unavailable while Dinoco transitions to the schema.bin-based migration flow.".white()
    );
    println!(
        "  {} {}",
        "→".cyan().bold(),
        "Use `migrate generate`, `migrate run`, and `models generate` with the new migration flow for now.".dimmed()
    );

    Ok(())
}
