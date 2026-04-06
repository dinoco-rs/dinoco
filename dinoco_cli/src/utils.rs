use std::env;

#[macro_export]
macro_rules! ternary {
    ($cond:expr, $a:expr, $b:expr) => {
        if $cond { $a } else { $b }
    };
}

pub fn env_prompt_bool(key: &str) -> Option<bool> {
    let value = env::var(key).ok()?;

    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" => Some(true),
        "0" | "false" | "no" | "n" => Some(false),
        _ => None,
    }
}

pub fn env_prompt_string(key: &str) -> Option<String> {
    env::var(key).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}
