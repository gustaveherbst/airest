use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use serde_json::Value;

use crate::config::{load_env, Config};
use crate::definitions::{
    validate_api_dir, validate_api_file, EndpointDefinition, LoadOptions,
};
use crate::openapi::generate_openapi;
use crate::server;
use crate::style;
use crate::templates;
use crate::validation::validate_input;

#[derive(Parser)]
#[command(
    name = "airest",
    about = "Declarative AI REST API framework",
    color = clap::ColorChoice::Auto
)]
pub struct Cli {
    /// Load environment variables from this file instead of `.env` in the current directory
    #[arg(long = "env-file", alias = "env", global = true, value_name = "PATH")]
    pub env_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the aiREST HTTP server
    Serve {
        /// Directory containing aiREST definition files (loads all .yaml/.yml)
        #[arg(long, alias = "folder")]
        dir: Option<PathBuf>,
        /// Only load definition files directly in --dir, not subfolders
        #[arg(long)]
        no_recursive: bool,
        /// Watch YAML folder and reload definitions (dev only; off by default)
        #[arg(long)]
        hot_reload: bool,
        /// Enforce production checks: real secrets, hot reload disabled
        #[arg(long)]
        production: bool,
    },
    /// Validate endpoint definition YAML files
    Validate {
        /// Directory containing aiREST definition files
        #[arg(long, alias = "folder")]
        dir: Option<PathBuf>,
        /// Validate a single definition file
        #[arg(long)]
        file: Option<PathBuf>,
        /// Only validate files directly in --dir, not subfolders
        #[arg(long)]
        no_recursive: bool,
    },
    /// Create a new endpoint definition from the default template
    New {
        name: String,
        #[arg(long, alias = "folder")]
        dir: Option<PathBuf>,
        #[arg(long)]
        category: Option<String>,
    },
    /// Call an endpoint with an example or custom JSON input
    Test {
        name: String,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long, default_value = "http://localhost:3300")]
        url: String,
        #[arg(long, alias = "folder")]
        dir: Option<PathBuf>,
        #[arg(long)]
        no_recursive: bool,
    },
    /// Show semantic/exact cache statistics
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
    /// Generate OpenAPI 3 documentation from endpoint definitions
    Openapi {
        #[arg(long, alias = "folder")]
        dir: Option<PathBuf>,
        #[arg(long, default_value = "openapi.json")]
        output: PathBuf,
        #[arg(long, default_value = "http://localhost:3300")]
        base_url: String,
        #[arg(long)]
        no_recursive: bool,
    },
}

#[derive(Subcommand)]
pub enum CacheCommands {
    /// Print cache entry counts and persistence status
    Stats,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    load_env(cli.env_file.as_deref())?;

    let Some(command) = cli.command else {
        eprintln!("{} no command specified.\n", style::emphasis("error:"));
        let mut cmd = Cli::command();
        cmd.print_help()?;
        eprintln!();
        std::process::exit(2);
    };

    match command {
        Commands::Serve {
            dir,
            no_recursive,
            hot_reload,
            production,
        } => {
            let mut config = Config::from_env()?;
            if let Some(dir) = dir {
                config.api_dir = dir;
            }
            config.load_recursive = !no_recursive;
            if hot_reload {
                config.hot_reload = true;
            }
            if production {
                config.production_mode = true;
            }
            server::start(config).await?;
        }
        Commands::Validate {
            dir,
            file,
            no_recursive,
        } => {
            run_validate(dir, file, no_recursive)?;
        }
        Commands::New {
            name,
            dir,
            category,
        } => {
            let config = Config::from_env_cli()?;
            let api_dir = dir.unwrap_or(config.api_dir);
            let path = templates::create_endpoint(&name, category.as_deref(), &api_dir)?;
            println!(
                "{} {}",
                style::success("Created endpoint definition:"),
                style::file_path(&path.display().to_string())
            );
        }
        Commands::Test {
            name,
            input,
            url,
            dir,
            no_recursive,
        } => {
            run_test(name, input, url, dir, no_recursive).await?;
        }
        Commands::Cache { command } => match command {
            CacheCommands::Stats => run_cache_stats()?,
        },
        Commands::Openapi {
            dir,
            output,
            base_url,
            no_recursive,
        } => {
            let config = Config::from_env_cli()?;
            let api_dir = dir.unwrap_or(config.api_dir);
            let options = load_options(!no_recursive);
            let endpoints = validate_api_dir(&api_dir, options)?;
            let spec = generate_openapi(&endpoints, &base_url);
            let json = serde_json::to_string_pretty(&spec)?;
            std::fs::write(&output, json)
                .with_context(|| format!("Failed to write OpenAPI file: {}", output.display()))?;
            println!(
                "{} OpenAPI spec for {} endpoint(s): {}",
                style::success("Generated"),
                style::count(endpoints.len()),
                style::file_path(&output.display().to_string())
            );
        }
    }

    Ok(())
}

fn run_cache_stats() -> Result<()> {
    let config = Config::from_env_cli()?;
    let store = crate::cache::CacheStore::new(&config);
    let stats = store.stats();
    println!(
        "{} exact={} semantic={} scopes={} persistent={}",
        style::success("Cache stats:"),
        style::count(stats.exact_entries),
        style::count(stats.vector_entries),
        style::count(stats.scopes),
        if stats.persistent { "yes" } else { "no" }
    );
    if let Some(path) = stats.store_path {
        println!("  store: {}", style::file_path(&path));
    }
    println!(
        "  max per endpoint: {}",
        style::count(config.cache_max_entries)
    );
    println!(
        "  hit rate: {:.1}%",
        store.vector.hit_rate() * 100.0
    );
    Ok(())
}

fn run_validate(dir: Option<PathBuf>, file: Option<PathBuf>, no_recursive: bool) -> Result<()> {
    if let Some(file) = file {
        let definition = validate_api_file(&file)?;
        print_validated_endpoint(&definition, Some(&file));
        println!("{}", style::success("Validated 1 endpoint definition file."));
        return Ok(());
    }

    let config = Config::from_env_cli()?;
    let api_dir = dir.unwrap_or(config.api_dir);
    let options = load_options(!no_recursive);
    let endpoints = validate_api_dir(&api_dir, options)?;
    println!(
        "Validated {} endpoint definition(s) in {}",
        style::count(endpoints.len()),
        style::file_path(&api_dir.display().to_string())
    );
    for endpoint in endpoints {
        print_validated_endpoint(&endpoint, None);
    }
    Ok(())
}

fn print_validated_endpoint(endpoint: &EndpointDefinition, file: Option<&PathBuf>) {
    let label = endpoint.display_label();
    if let Some(file) = file {
        println!(
            "  {}  {} {} {} {}",
            style::ok_tag(),
            style::label(&label),
            style::http_method(&endpoint.method),
            style::route(&endpoint.path),
            style::dim(&format!("({})", file.display()))
        );
    } else {
        println!(
            "  {}  {} {} {}",
            style::ok_tag(),
            style::label(&label),
            style::http_method(&endpoint.method),
            style::route(&endpoint.path)
        );
    }
}

async fn run_test(
    name: String,
    input: Option<PathBuf>,
    url: String,
    dir: Option<PathBuf>,
    no_recursive: bool,
) -> Result<()> {
    let config = Config::from_env_cli()?;
    let server_api_key = config.api_key().map(str::to_string);
    let api_dir = dir.unwrap_or(config.api_dir);
    let options = load_options(!no_recursive);
    let endpoints = validate_api_dir(&api_dir, options)?;
    let endpoint = endpoints
        .into_iter()
        .find(|def| def.name == name)
        .with_context(|| format!("No endpoint definition named '{name}'"))?;

    let payload = load_test_input(&endpoint, input)?;
    if let Err(err) = validate_input(&endpoint.input_schema, &payload) {
        bail!("Input validation failed before request: {err}");
    }

    let client = reqwest::Client::new();
    let request_url = format!("{}{}", url.trim_end_matches('/'), endpoint.path);
    let response = if endpoint.is_get() {
        let mut request = client.get(&request_url);
        if let Some(api_key) = server_api_key.as_deref() {
            if endpoint.auth_required() {
                request = request.header("x-api-key", api_key);
            }
        }
        request.query(&json_to_query_pairs(&payload)).send().await
    } else {
        let mut request = client.post(&request_url).json(&payload);
        if let Some(api_key) = server_api_key.as_deref() {
            if endpoint.auth_required() {
                request = request.header("x-api-key", api_key);
            }
        }
        request.send().await
    }
    .context("Failed to call aiREST endpoint")?;
    let status = response.status();
    let body: Value = response.json().await.context("Failed to parse response JSON")?;

    println!(
        "{} {} {} {} {}",
        style::http_method(&endpoint.method),
        style::route(&endpoint.path),
        style::label(&endpoint.display_label()),
        style::arrow(),
        style::http_status(status.as_u16())
    );
    println!("{}", style::dim(&serde_json::to_string_pretty(&body)?));

    if !status.is_success() {
        bail!("Endpoint test failed with status {status}");
    }

    Ok(())
}

fn load_test_input(endpoint: &EndpointDefinition, input: Option<PathBuf>) -> Result<Value> {
    if let Some(path) = input {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read input file: {}", path.display()))?;
        return serde_json::from_str(&content)
            .with_context(|| format!("Input file is not valid JSON: {}", path.display()));
    }

    endpoint
        .examples
        .as_ref()
        .and_then(|examples| examples.request.clone())
        .context(
            "No --input file provided and endpoint definition has no examples.request block",
        )
}

fn load_options(recursive: bool) -> LoadOptions {
    LoadOptions { recursive }
}

fn json_to_query_pairs(value: &Value) -> Vec<(String, String)> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };

    object
        .iter()
        .flat_map(|(key, item)| value_to_query_pairs(key, item))
        .collect()
}

fn value_to_query_pairs(key: &str, value: &Value) -> Vec<(String, String)> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| scalar_to_string(item).map(|v| (key.to_string(), v)))
            .collect(),
        _ => scalar_to_string(value)
            .map(|v| vec![(key.to_string(), v)])
            .unwrap_or_default(),
    }
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Bool(v) => Some(v.to_string()),
        Value::Number(v) => Some(v.to_string()),
        Value::String(v) => Some(v.clone()),
        _ => None,
    }
}
