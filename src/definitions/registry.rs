use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};

use crate::definitions::{
    load_endpoint_definition_file, load_endpoint_definitions_with_options, EndpointDefinition,
    LoadedEndpoint, LoadOptions,
};
use crate::guardrails::{GuardrailChain, GuardrailEngine};

/// Runtime endpoint with compiled guardrail chain.
#[derive(Clone)]
pub struct ActiveEndpoint {
    pub definition: EndpointDefinition,
    pub guardrail_chain: Option<GuardrailChain>,
}

#[derive(Clone, Default)]
pub struct EndpointRegistry {
    inner: Arc<RwLock<RegistrySnapshot>>,
}

#[derive(Clone, Default)]
struct RegistrySnapshot {
    endpoints: Vec<ActiveEndpoint>,
    by_route: HashMap<String, ActiveEndpoint>,
}

fn route_key(method: &str, path: &str) -> String {
    format!("{}:{}", method.to_ascii_uppercase(), path)
}

impl EndpointRegistry {
    pub fn from_definitions(endpoints: Vec<EndpointDefinition>) -> Self {
        let loaded: Vec<LoadedEndpoint> = endpoints
            .into_iter()
            .map(|definition| LoadedEndpoint {
                definition,
                source_path: PathBuf::from("."),
            })
            .collect();
        Self::from_loaded(&loaded, &GuardrailEngine::new())
    }

    pub fn from_loaded(loaded: &[LoadedEndpoint], engine: &GuardrailEngine) -> Self {
        Self::from_loaded_result(loaded, engine).expect("guardrail compilation failed")
    }

    pub fn from_loaded_result(
        loaded: &[LoadedEndpoint],
        engine: &GuardrailEngine,
    ) -> Result<Self> {
        let mut endpoints = Vec::with_capacity(loaded.len());
        for item in loaded {
            let definition = item.definition.clone();
            let chain = if definition.guardrails.as_ref().is_some_and(|g| !g.is_empty()) {
                Some(GuardrailChain::compile(
                    definition.guardrails.as_ref().unwrap(),
                    item.source_path.parent(),
                    engine,
                )?)
            } else {
                None
            };
            endpoints.push(ActiveEndpoint {
                definition,
                guardrail_chain: chain,
            });
        }

        let registry = Self::default();
        registry.replace_active(endpoints);
        Ok(registry)
    }

    fn replace_active(&self, endpoints: Vec<ActiveEndpoint>) {
        let by_route = endpoints
            .iter()
            .map(|active| (active.definition.route_key(), active.clone()))
            .collect();
        let mut guard = self.inner.write().expect("endpoint registry lock poisoned");
        guard.endpoints = endpoints;
        guard.by_route = by_route;
    }

    pub fn replace(&self, endpoints: Vec<EndpointDefinition>) {
        self.replace_active(
            endpoints
                .into_iter()
                .map(|definition| ActiveEndpoint {
                    definition,
                    guardrail_chain: None,
                })
                .collect(),
        );
    }

    pub fn reload_from_dir(
        &self,
        dir: &PathBuf,
        options: LoadOptions,
        engine: &GuardrailEngine,
        cache: Option<&crate::cache::CacheStore>,
    ) -> Result<usize> {
        let loaded = load_endpoint_definitions_with_options(dir, options)?;
        let count = loaded.len();
        if let Some(cache) = cache {
            for item in &loaded {
                cache.sync_endpoint(&item.definition);
            }
        }
        let registry = Self::from_loaded_result(&loaded, engine)?;
        self.replace_active(registry.list_active());
        Ok(count)
    }

    pub fn list_active(&self) -> Vec<ActiveEndpoint> {
        self.inner
            .read()
            .expect("endpoint registry lock poisoned")
            .endpoints
            .clone()
    }

    pub fn list(&self) -> Vec<EndpointDefinition> {
        self.list_active()
            .into_iter()
            .map(|a| a.definition)
            .collect()
    }

    pub fn get_active_by_method_and_path(
        &self,
        method: &str,
        path: &str,
    ) -> Option<ActiveEndpoint> {
        self.inner
            .read()
            .expect("endpoint registry lock poisoned")
            .by_route
            .get(&route_key(method, path))
            .cloned()
    }

    pub fn get_by_path(&self, path: &str) -> Option<EndpointDefinition> {
        self.get_by_method_and_path("POST", path)
            .or_else(|| self.get_by_method_and_path("GET", path))
    }

    pub fn get_by_method_and_path(&self, method: &str, path: &str) -> Option<EndpointDefinition> {
        self.get_active_by_method_and_path(method, path)
            .map(|a| a.definition)
    }

    pub fn count(&self) -> usize {
        self.inner
            .read()
            .expect("endpoint registry lock poisoned")
            .endpoints
            .len()
    }
}

pub fn validate_api_dir(dir: &PathBuf, options: LoadOptions) -> Result<Vec<EndpointDefinition>> {
    load_endpoint_definitions_with_options(dir, options)
        .map(|loaded| loaded.into_iter().map(|l| l.definition).collect())
        .with_context(|| format!("Invalid API directory: {}", dir.display()))
}

pub fn validate_api_file(path: &PathBuf) -> Result<EndpointDefinition> {
    load_endpoint_definition_file(path)
}
