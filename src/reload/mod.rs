use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{error, info};

use crate::config::Config;
use crate::definitions::{EndpointRegistry, LoadOptions};
use crate::guardrails::GuardrailEngine;

pub fn spawn_hot_reload(
    config: Config,
    registry: EndpointRegistry,
    load_options: LoadOptions,
    guardrails: GuardrailEngine,
    cache: std::sync::Arc<crate::cache::CacheStore>,
) -> Result<()> {
    let api_dir = config.api_dir.clone();
    let registry_for_thread = registry.clone();
    let api_dir_for_thread = api_dir.clone();

    std::thread::spawn(move || {
        let (reload_tx, reload_rx) = std::sync::mpsc::channel();

        let mut watcher = match RecommendedWatcher::new(
            move |result: notify::Result<Event>| match result {
                Ok(event) if should_reload(&event.kind) => {
                    let _ = reload_tx.send(());
                }
                Ok(_) => {}
                Err(err) => error!(error = %err, "Endpoint watcher error"),
            },
            notify::Config::default(),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                error!(error = %err, "Failed to create endpoint watcher");
                return;
            }
        };

        if let Err(err) = watcher.watch(api_dir_for_thread.as_path(), RecursiveMode::Recursive) {
            error!(error = %err, "Failed to watch API directory");
            return;
        }

        loop {
            if reload_rx.recv().is_err() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
            while reload_rx.try_recv().is_ok() {}

            match registry_for_thread.reload_from_dir(
                &api_dir_for_thread,
                load_options,
                &guardrails,
                Some(cache.as_ref()),
            )
            {
                Ok(count) => info!(count, "Hot-reloaded aiREST endpoint definitions"),
                Err(err) => error!(error = %err, "Failed to hot-reload endpoint definitions"),
            }
        }
    });

    info!(dir = %api_dir.display(), "Hot reload enabled for endpoint definitions");
    Ok(())
}

fn should_reload(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}
