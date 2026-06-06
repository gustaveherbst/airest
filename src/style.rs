//! ANSI styling for CLI and server startup output.
//! Respects TTY detection and `NO_COLOR` / `CLICOLOR` conventions.

use owo_colors::OwoColorize;

pub fn banner(text: &str) -> String {
    text.bold().cyan().to_string()
}

pub fn brand(text: &str) -> String {
    text.bold().bright_cyan().to_string()
}

pub fn success(text: &str) -> String {
    text.green().to_string()
}

pub fn ok_tag() -> String {
    "OK".bold().green().to_string()
}

pub fn info(text: &str) -> String {
    text.bright_blue().to_string()
}

pub fn emphasis(text: &str) -> String {
    text.bold().white().to_string()
}

pub fn dim(text: &str) -> String {
    text.bright_black().to_string()
}

pub fn label(text: &str) -> String {
    format!("[{}]", text.bright_magenta())
}

pub fn http_method(method: &str) -> String {
    match method.to_ascii_uppercase().as_str() {
        "POST" => method.bold().yellow().to_string(),
        "GET" => method.bold().green().to_string(),
        "PUT" => method.bold().blue().to_string(),
        "DELETE" => method.bold().red().to_string(),
        _ => method.bold().white().to_string(),
    }
}

pub fn route(path: &str) -> String {
    path.cyan().to_string()
}

pub fn file_path(path: &str) -> String {
    path.underline().blue().to_string()
}

pub fn url(text: &str) -> String {
    text.underline().bright_cyan().to_string()
}

pub fn count(n: usize) -> String {
    n.to_string().bold().green().to_string()
}

pub fn http_status(status: u16) -> String {
    let text = status.to_string();
    if (200..300).contains(&status) {
        text.bold().green().to_string()
    } else if (400..500).contains(&status) {
        text.bold().yellow().to_string()
    } else {
        text.bold().red().to_string()
    }
}

pub fn arrow() -> String {
    "→".dimmed().to_string()
}

pub fn hint(text: &str) -> String {
    text.italic().bright_black().to_string()
}
