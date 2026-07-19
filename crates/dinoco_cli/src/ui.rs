use colored::Colorize;

pub fn success(message: impl AsRef<str>) {
    println!("{} {}", "✓".green().bold(), message.as_ref().bright_white());
}

pub fn info(message: impl AsRef<str>) {
    println!("{} {}", "•".cyan().bold(), message.as_ref().white());
}

pub fn warning(message: impl AsRef<str>) {
    println!("{} {}", "!".yellow().bold(), message.as_ref().yellow());
}
