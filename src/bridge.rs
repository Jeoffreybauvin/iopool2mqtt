//! Core bridge orchestrating polling from the iopool API and publishing to MQTT.

use std::collections::HashSet;

use tokio::sync::watch;
use tracing::{error, info, instrument, warn};

use crate::config::Config;
use crate::discovery::build_ha_discovery_messages;
use crate::error::Result;
use crate::iopool::IopoolApi;
use crate::mqtt::{build_pool_messages, build_pools_summary_message, MqttPublisher};

/// Orchestrates periodic or single-shot synchronization between the iopool API and MQTT.
pub struct Bridge<A, M> {
    config: Config,
    api: A,
    mqtt: M,
    discovered_pools: HashSet<String>,
}

impl<A, M> Bridge<A, M>
where
    A: IopoolApi,
    M: MqttPublisher,
{
    /// Creates a new [`Bridge`] instance with the provided configuration, API client, and MQTT publisher.
    #[must_use]
    pub fn new(config: Config, api: A, mqtt: M) -> Self {
        Self {
            config,
            api,
            mqtt,
            discovered_pools: HashSet::new(),
        }
    }

    /// Executes a single polling cycle: fetches pools from iopool and publishes discovery + state/metric topics.
    ///
    /// Returns the number of pools processed during this cycle.
    ///
    /// # Errors
    ///
    /// Returns an error if fetching pools from the iopool API fails or if publishing to MQTT fails.
    #[instrument(skip(self))]
    pub async fn poll_and_publish(&mut self) -> Result<usize> {
        let pools = self
            .api
            .fetch_target_pools(&self.config.iopool_pool_ids)
            .await?;

        if pools.is_empty() {
            warn!("No pools returned by the iopool API");
        } else {
            info!(pool_count = pools.len(), "Fetched pools from iopool API");
        }

        // Publish summary array on `<base_topic>/pools`
        let summary_msg = build_pools_summary_message(
            &pools,
            &self.config.mqtt_base_topic,
            self.config.mqtt_retain,
        )?;
        self.mqtt.publish(&summary_msg).await?;

        for pool in &pools {
            if self.config.ha_discovery_enabled && !self.discovered_pools.contains(&pool.id) {
                let discovery_msgs = build_ha_discovery_messages(
                    pool,
                    &self.config.mqtt_base_topic,
                    &self.config.ha_discovery_prefix,
                    self.config.mqtt_use_pool_title,
                )?;
                info!(
                    pool_id = %pool.id,
                    pool_title = %pool.title,
                    entities = discovery_msgs.len(),
                    "Publishing Home Assistant MQTT Auto-Discovery configuration"
                );
                self.mqtt.publish_batch(&discovery_msgs).await?;
                self.discovered_pools.insert(pool.id.clone());
            }

            let pool_msgs = build_pool_messages(
                pool,
                &self.config.mqtt_base_topic,
                self.config.mqtt_retain,
                self.config.mqtt_use_pool_title,
            )?;

            info!(
                pool_id = %pool.id,
                pool_title = %pool.title,
                topics_published = pool_msgs.len(),
                temperature = ?pool.latest_measure.as_ref().and_then(|m| m.temperature),
                ph = ?pool.latest_measure.as_ref().and_then(|m| m.ph),
                orp = ?pool.latest_measure.as_ref().and_then(|m| m.orp),
                "Published pool metrics to MQTT"
            );
            self.mqtt.publish_batch(&pool_msgs).await?;
        }

        Ok(pools.len())
    }

    /// Runs the bridge according to `config.run_once` and `config.poll_interval` until a shutdown signal is received.
    ///
    /// In `run_once` mode, any error during the single cycle is propagated after disconnecting cleanly.
    /// In daemon mode, transient errors are logged and retried on the next polling tick.
    ///
    /// # Errors
    ///
    /// Returns an error if `run_once` is enabled and the single polling cycle fails, or if disconnecting fails.
    pub async fn run(&mut self, mut shutdown_rx: watch::Receiver<bool>) -> Result<()> {
        if self.config.run_once {
            info!("Running in single-shot mode (RUN_ONCE=true)");
            let poll_res = self.poll_and_publish().await;
            let disconnect_res = self.mqtt.disconnect().await;
            poll_res?;
            disconnect_res?;
            return Ok(());
        }

        info!(
            poll_interval_secs = self.config.poll_interval.as_secs(),
            ha_discovery_enabled = self.config.ha_discovery_enabled,
            mqtt_base_topic = %self.config.mqtt_base_topic,
            "Starting iopool2mqtt daemon loop"
        );

        loop {
            if let Err(err) = self.poll_and_publish().await {
                error!(error = %err, "Failed to poll iopool API or publish to MQTT; will retry on next cycle");
            }

            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Shutdown signal received, disconnecting from MQTT broker");
                        break;
                    }
                }
                () = tokio::time::sleep(self.config.poll_interval) => {}
            }
        }

        self.mqtt.disconnect().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Advice, LatestMeasure, MqttMessage, Pool};
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Mutex;

    struct FakeIopoolApi {
        pools: Vec<Pool>,
    }

    impl IopoolApi for FakeIopoolApi {
        async fn fetch_pools(&self) -> Result<Vec<Pool>> {
            Ok(self.pools.clone())
        }

        async fn fetch_pool(&self, pool_id: &str) -> Result<Pool> {
            self.pools
                .iter()
                .find(|p| p.id == pool_id)
                .cloned()
                .ok_or_else(|| crate::error::Error::ApiStatus {
                    status: 404,
                    body: "Not found".to_owned(),
                })
        }
    }

    #[derive(Default)]
    struct RecordingPublisher {
        published: Mutex<Vec<MqttMessage>>,
        disconnected: Mutex<bool>,
    }

    impl MqttPublisher for RecordingPublisher {
        async fn publish(&self, message: &MqttMessage) -> Result<()> {
            self.published.lock().unwrap().push(message.clone());
            Ok(())
        }

        async fn disconnect(&self) -> Result<()> {
            *self.disconnected.lock().unwrap() = true;
            Ok(())
        }
    }

    fn sample_pool() -> Pool {
        Pool {
            id: "pool-123".to_owned(),
            title: "Ma Piscine".to_owned(),
            mode: Some("STANDARD".to_owned()),
            has_an_action_required: Some(false),
            latest_measure: Some(LatestMeasure {
                temperature: Some(25.5),
                ph: Some(7.4),
                orp: Some(700.0),
                mode: Some("gateway".to_owned()),
                is_valid: Some(true),
                eco_id: Some("eco1".to_owned()),
                gateway_code: Some("gw1".to_owned()),
                measured_at: Some("2026-10-23T10:00:00.000Z".to_owned()),
                extra: BTreeMap::new(),
            }),
            advice: Some(Advice {
                filtration_duration: Some(6.0),
                extra: BTreeMap::new(),
            }),
            salt: None,
            extra: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn test_bridge_run_once_publishes_discovery_and_metrics() {
        let mut env = HashMap::new();
        env.insert("IOPOOL_API_KEY", "test-key");
        env.insert("RUN_ONCE", "true");
        env.insert("HA_DISCOVERY_ENABLED", "true");
        let config = Config::from_map(&env).unwrap();

        let api = FakeIopoolApi {
            pools: vec![sample_pool()],
        };
        let publisher = RecordingPublisher::default();

        let mut bridge = Bridge::new(config, api, publisher);
        let (_tx, rx) = watch::channel(false);
        bridge.run(rx).await.unwrap();

        assert!(*bridge.mqtt.disconnected.lock().unwrap());
        let msgs = bridge.mqtt.published.lock().unwrap();

        // Check summary topic
        assert!(msgs.iter().any(|m| m.topic == "iopool2mqtt/pools"));
        // Check HA discovery topic
        assert!(msgs
            .iter()
            .any(|m| m.topic == "homeassistant/sensor/iopool_pool-123_temperature/config"));
        // Check pool state & individual topics
        assert!(msgs.iter().any(|m| m.topic == "iopool2mqtt/pool-123/state"));
        assert!(msgs.iter().any(
            |m| m.topic == "iopool2mqtt/pool-123/latestMeasure/temperature" && m.payload == "25.5"
        ));
    }

    #[tokio::test]
    async fn test_bridge_run_once_with_pool_title_enabled() {
        let mut env = HashMap::new();
        env.insert("IOPOOL_API_KEY", "test-key");
        env.insert("RUN_ONCE", "true");
        env.insert("HA_DISCOVERY_ENABLED", "true");
        env.insert("MQTT_USE_POOL_TITLE", "true");
        let config = Config::from_map(&env).unwrap();

        let api = FakeIopoolApi {
            pools: vec![sample_pool()],
        };
        let publisher = RecordingPublisher::default();

        let mut bridge = Bridge::new(config, api, publisher);
        let (_tx, rx) = watch::channel(false);
        bridge.run(rx).await.unwrap();

        let msgs = bridge.mqtt.published.lock().unwrap();

        // Pool title "Ma Piscine" is sanitized to "Ma_Piscine" in topic paths
        assert!(msgs
            .iter()
            .any(|m| m.topic == "iopool2mqtt/Ma_Piscine/state"));
        assert!(msgs.iter().any(
            |m| m.topic == "iopool2mqtt/Ma_Piscine/latestMeasure/temperature"
                && m.payload == "25.5"
        ));
        let ha_temp = msgs
            .iter()
            .find(|m| m.topic == "homeassistant/sensor/iopool_Ma_Piscine_temperature/config")
            .unwrap();
        assert!(ha_temp
            .payload
            .contains("\"name\":\"Ma Piscine Temperature\""));
        assert!(ha_temp
            .payload
            .contains("\"state_topic\":\"iopool2mqtt/Ma_Piscine/state\""));
    }
}
