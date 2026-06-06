use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use walkdir::WalkDir;

use super::loaded::LoadedEndpoint;
use super::types::EndpointDefinition;
use super::validator::validate_endpoint_definition;
use crate::config::DEFAULT_API_DIR;

#[derive(Debug, Clone, Copy)]
pub struct LoadOptions {
    /// When true, load YAML from subfolders as well.
    pub recursive: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self { recursive: true }
    }
}

pub fn load_endpoint_definitions(dir: &Path) -> Result<Vec<EndpointDefinition>> {
    Ok(load_endpoint_definitions_with_options(dir, LoadOptions::default())?
        .into_iter()
        .map(|loaded| loaded.definition)
        .collect())
}

pub fn load_endpoint_definitions_with_options(
    dir: &Path,
    options: LoadOptions,
) -> Result<Vec<LoadedEndpoint>> {
    if !dir.is_dir() {
        bail!("API definitions path is not a directory: {}", dir.display());
    }

    let mut definitions = Vec::new();
    let mut seen_routes = std::collections::HashMap::new();
    let mut seen_names = std::collections::HashMap::new();

    let mut walker = WalkDir::new(dir);
    if !options.recursive {
        walker = walker.max_depth(1);
    }

    for entry in walker
        .into_iter()
        .filter_entry(|entry| !should_skip_entry(entry.path(), dir))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Some(definition) = load_definition_file(path, false)? {
            if let Some(existing) =
                seen_routes.insert(definition.route_key(), path.display().to_string())
            {
                bail!(
                    "Duplicate endpoint route '{} {}': defined in both '{}' and '{}'",
                    definition.method,
                    definition.path,
                    existing,
                    path.display()
                );
            }

            if let Some(existing) =
                seen_names.insert(definition.name.clone(), path.display().to_string())
            {
                bail!(
                    "Duplicate endpoint name '{}': defined in both '{}' and '{}'",
                    definition.name,
                    existing,
                    path.display()
                );
            }

            definitions.push(LoadedEndpoint {
                definition,
                source_path: path.to_path_buf(),
            });
        }
    }

    if definitions.is_empty() {
        bail!(
            "No endpoint definitions found in {} (expected .yaml or .yml files)",
            dir.display()
        );
    }

    definitions.sort_by(|a, b| a.definition.path.cmp(&b.definition.path));
    Ok(definitions)
}

pub fn load_endpoint_definition_file(path: &Path) -> Result<EndpointDefinition> {
    load_definition_file(path, true)?
        .with_context(|| format!("Not a valid aiREST definition file: {}", path.display()))
}

fn should_skip_entry(path: &Path, root: &Path) -> bool {
    if path == root {
        return false;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.starts_with('.')
                || matches!(
                    name,
                    "target" | "node_modules" | "dist" | "build" | "templates"
                )
        })
        .unwrap_or(false)
}

fn load_definition_file(path: &Path, strict: bool) -> Result<Option<EndpointDefinition>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();

    if !matches!(ext, "yaml" | "yml") {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read definition file: {}", path.display()))?;

    let mut definition: EndpointDefinition = match serde_yaml::from_str(&content) {
        Ok(definition) => definition,
        Err(err) => {
            if strict {
                return Err(err).with_context(|| {
                    format!("Failed to parse YAML definition: {}", path.display())
                });
            }
            return Ok(None);
        }
    };

    resolve_local_tool_scripts(&mut definition, path.parent())?;

    if let Err(err) = validate_endpoint_definition(&definition) {
        if strict {
            return Err(err).with_context(|| {
                format!("Invalid endpoint definition in {}", path.display())
            });
        }
        return Ok(None);
    }

    Ok(Some(definition))
}

pub fn resolve_definitions_path(
    dir: Option<PathBuf>,
    file: Option<PathBuf>,
) -> Result<(PathBuf, LoadOptions)> {
    if let Some(file) = file {
        let parent = file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        return Ok((parent, LoadOptions { recursive: false }));
    }

    Ok((
        dir.unwrap_or_else(|| PathBuf::from(DEFAULT_API_DIR)),
        LoadOptions::default(),
    ))
}

fn resolve_local_tool_scripts(
    definition: &mut EndpointDefinition,
    definition_dir: Option<&Path>,
) -> Result<()> {
    let Some(tools) = definition.tools.as_mut() else {
        return Ok(());
    };
    let Some(local) = tools.local.as_mut() else {
        return Ok(());
    };

    for (index, spec) in local.iter_mut().enumerate() {
        if spec
            .script
            .as_ref()
            .is_some_and(|script| !script.trim().is_empty())
        {
            continue;
        }
        let Some(path) = spec.path.as_ref().filter(|p| !p.trim().is_empty()) else {
            continue;
        };
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            let base = definition_dir.ok_or_else(|| {
                anyhow::anyhow!(
                    "tools.local[{index}] path is relative but definition file directory is unknown"
                )
            })?;
            base.join(path)
        };
        let raw = std::fs::read_to_string(&resolved).with_context(|| {
            format!(
                "Failed to read tools.local[{index}] script at {}",
                resolved.display()
            )
        })?;
        spec.script = Some(crate::script::prepare_deno_script(&raw, Some(resolved.as_path())).map_err(
            |err| anyhow::anyhow!("tools.local[{index}] TypeScript transpile failed: {}", err.message()),
        )?);
    }
    Ok(())
}
