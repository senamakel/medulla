//! Subscription-usage meters sampled from local provider accounts.

mod claude;
mod codex;
mod openrouter;
mod time;
mod tinyhumans;
mod types;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::config::{MeterId, MeterSource, ResourceDisplay, SubscriptionsConfig};
pub use openrouter::DEFAULT_KEY_ENV as OPENROUTER_KEY_ENV;
pub use types::{MeterReading, SubscriptionSnapshot};

/// Sample every switched-on meter.
pub async fn sample(
    config: &SubscriptionsConfig,
    account_usage: Option<&Value>,
) -> SubscriptionSnapshot {
    let mut meters = BTreeMap::new();
    if !config.any_enabled() {
        return SubscriptionSnapshot {
            meters,
            sampled_at: now_seconds(),
        };
    }
    let (claude, open_router) = futures::join!(
        async {
            match config.source_enabled(MeterSource::Claude) {
                true => Some(claude::sample().await),
                false => None,
            }
        },
        async {
            match config.source_enabled(MeterSource::OpenRouter) {
                true => Some(openrouter::sample(config.open_router_key_env.as_deref()).await),
                false => None,
            }
        },
    );
    if let Some(result) = claude {
        absorb(&mut meters, config, MeterSource::Claude, result);
    }
    if let Some(result) = open_router {
        absorb(
            &mut meters,
            config,
            MeterSource::OpenRouter,
            result.map(|reading| vec![(MeterId::OpenRouterCredits, reading)]),
        );
    }
    if config.source_enabled(MeterSource::Codex) {
        let result = tokio::task::spawn_blocking(codex::sample)
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
        absorb(&mut meters, config, MeterSource::Codex, result);
    }
    if config.source_enabled(MeterSource::Tinyhumans) {
        let result = match account_usage {
            Some(usage) => tinyhumans::reading(usage)
                .map(|reading| vec![(MeterId::TinyhumansBalance, reading)])
                .ok_or_else(|| "no portal balance on this account".to_string()),
            None => Err("account usage not loaded".to_string()),
        };
        absorb(&mut meters, config, MeterSource::Tinyhumans, result);
    }
    SubscriptionSnapshot {
        meters,
        sampled_at: now_seconds(),
    }
}

fn absorb(
    meters: &mut BTreeMap<MeterId, MeterReading>,
    config: &SubscriptionsConfig,
    source: MeterSource,
    result: Result<Vec<(MeterId, MeterReading)>, String>,
) {
    let enabled = MeterId::ALL
        .into_iter()
        .filter(|meter| meter.source() == source && config.display(*meter) != ResourceDisplay::Off);
    match result {
        Ok(readings) => {
            for meter in enabled {
                let reading = readings
                    .iter()
                    .find(|(id, _)| *id == meter)
                    .map(|(_, reading)| reading.clone())
                    .unwrap_or_else(|| MeterReading::unavailable("not reported for this plan"));
                meters.insert(meter, reading);
            }
        }
        Err(reason) => {
            for meter in enabled {
                meters.insert(meter, MeterReading::unavailable(reason.clone()));
            }
        }
    }
}

fn now_seconds() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
