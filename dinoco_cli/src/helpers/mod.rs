use chrono::{DateTime, Utc};
use dinoco_derives::Rowable;

mod database;
mod migrations;

pub use database::*;
pub use migrations::*;

#[derive(Rowable, Debug)]
pub struct DinocoMigration {
    pub name: String,
    pub applied_at: Option<DateTime<Utc>>,
    pub rollback_at: Option<DateTime<Utc>>,
}

pub fn to_snake_case(s: &str) -> String {
    let mut snake = String::new();

    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() {
            if i > 0 && !snake.ends_with('_') {
                snake.push('_');
            }

            snake.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            if !snake.ends_with('_') {
                snake.push('_');
            }
        } else {
            snake.push(ch);
        }
    }

    snake.trim_matches('_').to_string()
}
