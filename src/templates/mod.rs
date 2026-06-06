use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

const DEFAULT_TEMPLATE: &str = include_str!("../../templates/default-endpoint.yaml");

pub fn create_endpoint(name: &str, category: Option<&str>, dir: &Path) -> Result<PathBuf> {
    if name.trim().is_empty() {
        bail!("Endpoint name cannot be empty");
    }

    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create API directory: {}", dir.display()))?;

    let slug = slugify(name);
    let path = dir.join(format!("{slug}.yaml"));

    if path.exists() {
        bail!("Endpoint file already exists: {}", path.display());
    }

    let category_line = category
        .map(|value| format!("category: {value}\n"))
        .unwrap_or_default();

    let content = DEFAULT_TEMPLATE
        .replace("__NAME__", name)
        .replace("__CATEGORY_LINE__", &category_line)
        .replace("__SLUG__", &slug)
        .replace("__PATH__", &format!("/v1/{slug}"));

    fs::write(&path, content)
        .with_context(|| format!("Failed to write endpoint template: {}", path.display()))?;

    Ok(path)
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
