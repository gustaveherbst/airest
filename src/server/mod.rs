use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde_json::Value;
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::auth::{verify_request, JtiDenylist, JwksCache};
use crate::cache::CacheStore;
use crate::config::production::validate_production_config;
use crate::config::Config;
use crate::definitions::{
    load_endpoint_definitions_with_options, validate_provider_credentials, ActiveEndpoint,
    EndpointRegistry, LoadOptions,
};
use crate::errors::{AiRestError, ErrorResponse, ErrorType};
use crate::guardrails::GuardrailEngine;
use crate::health::{endpoint_health_response, global_health_response, readiness_response};
use crate::llm::{CircuitBreakerConfig, LlmRouter};
use crate::openapi::generate_openapi;
use crate::otel::{self, request as request_span, TelemetryState};
use crate::reload::spawn_hot_reload;
use crate::runtime::{execute_request, new_request_id, ExecutionContext};
use crate::style;
use crate::validation::query_params_to_input;
use tracing::Instrument;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub llm: Arc<LlmRouter>,
    pub registry: EndpointRegistry,
    pub cache: Arc<CacheStore>,
    pub jwks: Arc<JwksCache>,
    pub jti_denylist: Arc<JtiDenylist>,
    pub http: Arc<Client>,
    pub guardrails: GuardrailEngine,
    pub telemetry: TelemetryState,
    pub concurrency: Arc<Semaphore>,
    pub accepting_requests: Arc<AtomicBool>,
    pub in_flight: Arc<AtomicUsize>,
}

struct InFlightGuard {
    counter: Arc<AtomicUsize>,
}

impl InFlightGuard {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self { counter }
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

impl AppState {
    pub async fn build(
        config: Arc<Config>,
        registry: EndpointRegistry,
        guardrails: GuardrailEngine,
    ) -> anyhow::Result<Self> {
        let cache = Arc::new(CacheStore::new(&config));
        for active in registry.list_active() {
            cache.sync_endpoint(&active.definition);
        }

        let circuit_config = CircuitBreakerConfig {
            failure_threshold: config.llm_circuit_breaker_threshold,
            reset_after: Duration::from_secs(config.llm_circuit_breaker_reset_secs),
        };

        let telemetry = TelemetryState::from_config(&config);

        Ok(Self {
            llm: Arc::new(LlmRouter::with_circuit_breaker(
                config.providers.clone(),
                circuit_config,
            )),
            jti_denylist: Arc::new(JtiDenylist::from_config(&config).await),
            concurrency: Arc::new(Semaphore::new(config.max_concurrent_requests.max(1))),
            accepting_requests: Arc::new(AtomicBool::new(true)),
            in_flight: Arc::new(AtomicUsize::new(0)),
            config,
            registry,
            cache,
            jwks: Arc::new(JwksCache::default()),
            http: Arc::new(Client::new()),
            guardrails,
            telemetry,
        })
    }
}

pub async fn start(config: Config) -> anyhow::Result<()> {
    let load_options = LoadOptions {
        recursive: config.load_recursive,
    };
    let loaded =
        load_endpoint_definitions_with_options(&config.api_dir, load_options.clone())?;
    let definitions: Vec<_> = loaded.iter().map(|l| l.definition.clone()).collect();

    // First check: required provider credentials must be present before any server init.
    if let Err(err) = validate_provider_credentials(&config.providers, &definitions) {
        exit_startup_warning(err);
    }
    validate_production_config(&config, &config.providers, &definitions)?;

    otel::init_tracing(&config);
    let endpoint_count = loaded.len();
    let guardrails = GuardrailEngine::new();
    let registry = EndpointRegistry::from_loaded_result(&loaded, &guardrails)?;

    let config = Arc::new(config);
    let mut state = AppState::build(config.clone(), registry.clone(), guardrails.clone()).await?;
    state.telemetry = TelemetryState::from_config(&config);

    if config.hot_reload {
        spawn_hot_reload(
            (*config).clone(),
            registry,
            load_options,
            guardrails,
            state.cache.clone(),
        )?;
    }

    println!(
        "{} {}",
        style::banner("aiREST server started on"),
        style::url(&format!("http://localhost:{}", config.port))
    );
    println!(
        "Loaded {} aiREST endpoint definition(s) from {}.",
        style::count(endpoint_count),
        style::file_path(&config.api_dir.display().to_string())
    );
    for endpoint in state.registry.list() {
        println!(
            "  {}  {} {} {}",
            style::dim("Registered"),
            style::http_method(&endpoint.method),
            style::route(&endpoint.path),
            style::label(&endpoint.display_label())
        );
    }
    if config.hot_reload {
        println!(
            "{} {}",
            style::info("Hot reload enabled for"),
            style::file_path(&config.api_dir.display().to_string())
        );
    } else {
        println!(
            "{} {}",
            style::dim("Hot reload"),
            style::label("disabled (set AIREST_HOT_RELOAD=true to enable)")
        );
    }
    if config.production_mode {
        println!(
            "{} {}",
            style::info("Production mode"),
            style::label("enabled")
        );
    }

    let app = build_router(state.clone(), &config);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    let accepting = state.accepting_requests.clone();
    let in_flight = state.in_flight.clone();
    let shutdown_timeout = if config.graceful_shutdown_enabled {
        Duration::from_secs(config.graceful_shutdown_secs)
    } else {
        Duration::ZERO
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(
            accepting,
            in_flight,
            shutdown_timeout,
            config.graceful_shutdown_enabled,
        ))
        .await?;
    Ok(())
}

fn exit_startup_warning(err: impl std::fmt::Display) -> ! {
    eprintln!("{} {:#}", style::info("Warning:"), err);
    std::process::exit(1);
}

async fn shutdown_signal(
    accepting: Arc<AtomicBool>,
    in_flight: Arc<AtomicUsize>,
    drain_timeout: Duration,
    graceful_shutdown_enabled: bool,
) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    accepting.store(false, Ordering::SeqCst);

    if !graceful_shutdown_enabled || drain_timeout.is_zero() {
        tracing::info!("Shutdown signal received; graceful drain disabled, exiting immediately");
        return;
    }

    drain_in_flight(in_flight, drain_timeout).await;
}

async fn drain_in_flight(in_flight: Arc<AtomicUsize>, drain_timeout: Duration) {
    if in_flight.load(Ordering::SeqCst) == 0 {
        tracing::info!("Shutdown signal received; no in-flight requests, exiting immediately");
        return;
    }

    tracing::info!(
        in_flight = in_flight.load(Ordering::SeqCst),
        drain_timeout_secs = drain_timeout.as_secs(),
        "Shutdown signal received; draining in-flight requests"
    );

    let deadline = tokio::time::Instant::now() + drain_timeout;
    let poll_interval = Duration::from_millis(100);

    loop {
        let active = in_flight.load(Ordering::SeqCst);
        if active == 0 {
            tracing::info!("All in-flight requests completed; shutdown complete");
            return;
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(
                in_flight = active,
                drain_timeout_secs = drain_timeout.as_secs(),
                "Shutdown drain timeout elapsed with requests still in flight"
            );
            return;
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub fn build_router(state: AppState, config: &Config) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        .route("/openapi.json", get(openapi_handler))
        .fallback(dynamic_handler)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(config.request_timeout_secs.max(1)),
        ))
        .layer(RequestBodyLimitLayer::new(
            config.max_request_body_bytes.max(1024),
        ))
        .with_state(state)
}

pub fn build_router_from_state(state: AppState) -> Router {
    build_router(state.clone(), &state.config)
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    global_health_response(&state.registry)
}

async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    readiness_response(
        &state.registry,
        &state.config.providers,
        state.llm.as_ref(),
        state.accepting_requests.load(Ordering::SeqCst),
    )
}

async fn openapi_handler(State(state): State<AppState>) -> impl IntoResponse {
    let base_url = format!("http://localhost:{}", state.config.port);
    let spec = generate_openapi(&state.registry.list(), &base_url);
    Json(spec)
}

async fn dynamic_handler(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ErrorResponse> {
    if uri.path() == "/openapi.json" {
        return Err(not_found_response(method, uri));
    }

    let body_bytes = body.len();

    if method == Method::GET {
        if let Some(base_path) = uri.path().strip_suffix("/health") {
            if !base_path.is_empty() {
                if let Some(active) = state
                    .registry
                    .get_active_by_method_and_path("GET", base_path)
                    .or_else(|| state.registry.get_active_by_method_and_path("POST", base_path))
                {
                    return Ok(endpoint_health_response(&active.definition));
                }
            }
        }

        let Some(active) = state.registry.get_active_by_method_and_path("GET", uri.path()) else {
            return Err(not_found_response(method, uri));
        };

        let input = match query_params_to_input(
            &active.definition.input_schema,
            uri.query().as_deref(),
        ) {
            Ok(value) => value,
            Err(err) => {
                return Err(ErrorResponse::for_endpoint(
                    new_request_id(),
                    &active.definition,
                    AiRestError::with_details(
                        ErrorType::InputValidation,
                        "Request query parameters could not be parsed for input schema.",
                        serde_json::json!({ "reason": err.to_string() }),
                    ),
                ));
            }
        };

        return handle_endpoint(state, active, headers, input, body_bytes)
            .await
            .map(|response| response.into_response());
    }

    if method != Method::POST {
        return Err(not_found_response(method, uri));
    }

    let Some(active) = state.registry.get_active_by_method_and_path("POST", uri.path()) else {
        return Err(not_found_response(method, uri));
    };

    let input: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            return Err(ErrorResponse::for_endpoint(
                new_request_id(),
                &active.definition,
                AiRestError::with_details(
                    ErrorType::InputValidation,
                    "Request body must be valid JSON.",
                    serde_json::json!({ "reason": err.to_string() }),
                ),
            ));
        }
    };

    handle_endpoint(state, active, headers, input, body_bytes)
        .await
        .map(|response| response.into_response())
}

fn not_found_response(method: Method, uri: Uri) -> ErrorResponse {
    ErrorResponse::new(
        new_request_id(),
        AiRestError::with_details(
            ErrorType::NotFound,
            "No matching aiREST endpoint.",
            serde_json::json!({
                "method": method.as_str(),
                "path": uri.path(),
            }),
        ),
    )
}

async fn handle_endpoint(
    state: AppState,
    active: ActiveEndpoint,
    headers: HeaderMap,
    input: Value,
    request_body_bytes: usize,
) -> Result<impl IntoResponse, ErrorResponse> {
    if !state.accepting_requests.load(Ordering::SeqCst) {
        return Err(ErrorResponse::new(
            new_request_id(),
            AiRestError::new(
                ErrorType::InternalServer,
                "Server is shutting down.",
            ),
        ));
    }

    let _in_flight = InFlightGuard::new(state.in_flight.clone());

    let _permit = state
        .concurrency
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| {
            ErrorResponse::new(
                new_request_id(),
                AiRestError::new(
                    ErrorType::InternalServer,
                    "Server concurrency limit unavailable.",
                ),
            )
        })?;

    let started = std::time::Instant::now();
    let request_id = new_request_id();
    let endpoint = &active.definition;

    let auth = match verify_request(
        &state.config,
        endpoint,
        &headers,
        state.jwks.as_ref(),
        state.http.as_ref(),
        state.jti_denylist.as_ref(),
    )
    .await
    {
        Ok(ctx) => ctx,
        Err(err) => return Err(ErrorResponse::for_endpoint(request_id, endpoint, err)),
    };

    let cache = if state.config.cache_enabled || endpoint.cache.as_ref().is_some_and(|c| c.enabled) {
        Some(state.cache.as_ref())
    } else {
        None
    };

    if state.telemetry.endpoint_enabled(endpoint) {
        state.telemetry.request_started(&endpoint.name);
    }

    let span = request_span(
        &endpoint.name,
        &endpoint.version,
        &request_id,
        endpoint.method.as_str(),
        &endpoint.path,
        auth.as_ref(),
    );
    let result = async {
        execute_request(ExecutionContext {
            config: &state.config,
            llm: &state.llm,
            endpoint,
            request_id: request_id.clone(),
            input,
            request_body_bytes,
            auth,
            cache,
            guardrail_chain: active.guardrail_chain.as_ref(),
            telemetry: Some(&state.telemetry),
        })
        .await
    }
    .instrument(span)
    .await;

    if state.telemetry.endpoint_enabled(endpoint) {
        let status = if result.is_ok() { "ok" } else { "error" };
        state.telemetry.record_request(
            &endpoint.name,
            status,
            started.elapsed().as_millis() as u64,
        );
        state.telemetry.request_finished(&endpoint.name);
    }

    let result = result?;

    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(result.success).map_err(|_| {
            ErrorResponse::for_endpoint(
                request_id,
                endpoint,
                AiRestError::new(
                    ErrorType::InternalServer,
                    "Failed to serialize success response",
                ),
            )
        })?),
    ))
}
