//! Asynchronous MQTT publisher and topic/payload formatting logic.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rumqttc::{AsyncClient, ConnectionError, Event, LastWill, MqttOptions, Outgoing, Packet, QoS};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

use crate::config::Config;
use crate::error::Result;
use crate::models::{format_json_value, format_number, MqttMessage, Pool};

/// Trait abstracting an MQTT publisher so the bridge logic can be tested without a live broker.
#[allow(async_fn_in_trait)]
pub trait MqttPublisher: Send + Sync {
    /// Publishes a single [`MqttMessage`] to the broker.
    async fn publish(&self, message: &MqttMessage) -> Result<()>;

    /// Publishes a batch of [`MqttMessage`] items in order.
    async fn publish_batch(&self, messages: &[MqttMessage]) -> Result<()> {
        for message in messages {
            self.publish(message).await?;
        }
        Ok(())
    }

    /// Publishes the offline status (if applicable), flushes pending packets, and disconnects cleanly.
    async fn disconnect(&self) -> Result<()>;
}

/// Builds all MQTT messages (full JSON state + individual sub-topics) for a single [`Pool`].
///
/// # Generated Topics
///
/// Given `base_topic = "iopool2mqtt"` and `pool.id = "<pool_id>"`:
/// - `iopool2mqtt/<pool_id>/state` (full JSON object)
/// - `iopool2mqtt/<pool_id>/id`
/// - `iopool2mqtt/<pool_id>/title`
/// - `iopool2mqtt/<pool_id>/mode`
/// - `iopool2mqtt/<pool_id>/hasAnActionRequired`
/// - `iopool2mqtt/<pool_id>/salt`
/// - `iopool2mqtt/<pool_id>/advice/filtrationDuration`
/// - `iopool2mqtt/<pool_id>/latestMeasure` (JSON object of `latestMeasure`)
/// - `iopool2mqtt/<pool_id>/latestMeasure/temperature`
/// - `iopool2mqtt/<pool_id>/latestMeasure/ph`
/// - `iopool2mqtt/<pool_id>/latestMeasure/orp`
/// - `iopool2mqtt/<pool_id>/latestMeasure/mode`
/// - `iopool2mqtt/<pool_id>/latestMeasure/isValid`
/// - `iopool2mqtt/<pool_id>/latestMeasure/ecoId`
/// - `iopool2mqtt/<pool_id>/latestMeasure/gatewayCode`
/// - `iopool2mqtt/<pool_id>/latestMeasure/measuredAt`
///
/// # Errors
///
/// Returns [`crate::error::Error::Json`] if JSON serialization fails.
pub fn build_pool_messages(
    pool: &Pool,
    base_topic: &str,
    retain: bool,
) -> Result<Vec<MqttMessage>> {
    let prefix = format!("{base_topic}/{}", pool.id);
    let mut messages = Vec::with_capacity(20);

    // 1. Full JSON state payload for the pool
    let full_json = serde_json::to_string(pool)?;
    messages.push(MqttMessage::new(
        format!("{prefix}/state"),
        full_json,
        retain,
    ));

    // 2. Top-level scalar attributes
    messages.push(MqttMessage::new(
        format!("{prefix}/id"),
        pool.id.clone(),
        retain,
    ));
    messages.push(MqttMessage::new(
        format!("{prefix}/title"),
        pool.title.clone(),
        retain,
    ));

    if let Some(ref mode) = pool.mode {
        messages.push(MqttMessage::new(
            format!("{prefix}/mode"),
            mode.clone(),
            retain,
        ));
    }

    if let Some(action_req) = pool.has_an_action_required {
        messages.push(MqttMessage::new(
            format!("{prefix}/hasAnActionRequired"),
            action_req.to_string(),
            retain,
        ));
    }

    let salt_payload = match &pool.salt {
        Some(v) => format_json_value(v),
        None => "null".to_owned(),
    };
    messages.push(MqttMessage::new(
        format!("{prefix}/salt"),
        salt_payload,
        retain,
    ));

    // 3. Advice sub-tree
    if let Some(ref advice) = pool.advice {
        let advice_json = serde_json::to_string(advice)?;
        messages.push(MqttMessage::new(
            format!("{prefix}/advice"),
            advice_json,
            retain,
        ));

        if let Some(duration) = advice.filtration_duration {
            messages.push(MqttMessage::new(
                format!("{prefix}/advice/filtrationDuration"),
                format_number(duration),
                retain,
            ));
        }

        for (k, v) in &advice.extra {
            messages.push(MqttMessage::new(
                format!("{prefix}/advice/{k}"),
                format_json_value(v),
                retain,
            ));
        }
    }

    // 4. LatestMeasure sub-tree
    if let Some(ref measure) = pool.latest_measure {
        let measure_json = serde_json::to_string(measure)?;
        messages.push(MqttMessage::new(
            format!("{prefix}/latestMeasure"),
            measure_json,
            retain,
        ));

        if let Some(temp) = measure.temperature {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/temperature"),
                format_number(temp),
                retain,
            ));
        }
        if let Some(ph) = measure.ph {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/ph"),
                format_number(ph),
                retain,
            ));
        }
        if let Some(orp) = measure.orp {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/orp"),
                format_number(orp),
                retain,
            ));
        }
        if let Some(ref mode) = measure.mode {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/mode"),
                mode.clone(),
                retain,
            ));
        }
        if let Some(is_valid) = measure.is_valid {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/isValid"),
                is_valid.to_string(),
                retain,
            ));
        }
        if let Some(ref eco_id) = measure.eco_id {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/ecoId"),
                eco_id.clone(),
                retain,
            ));
        }
        if let Some(ref gateway_code) = measure.gateway_code {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/gatewayCode"),
                gateway_code.clone(),
                retain,
            ));
        }
        if let Some(ref measured_at) = measure.measured_at {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/measuredAt"),
                measured_at.clone(),
                retain,
            ));
        }

        for (k, v) in &measure.extra {
            messages.push(MqttMessage::new(
                format!("{prefix}/latestMeasure/{k}"),
                format_json_value(v),
                retain,
            ));
        }
    }

    // 5. Any extra top-level fields on Pool
    for (k, v) in &pool.extra {
        messages.push(MqttMessage::new(
            format!("{prefix}/{k}"),
            format_json_value(v),
            retain,
        ));
    }

    Ok(messages)
}

/// Builds a summary message on `<base_topic>/pools` containing the JSON array of all pools.
///
/// # Errors
///
/// Returns [`crate::error::Error::Json`] if serialization fails.
pub fn build_pools_summary_message(
    pools: &[Pool],
    base_topic: &str,
    retain: bool,
) -> Result<MqttMessage> {
    let payload = serde_json::to_string(pools)?;
    Ok(MqttMessage::new(
        format!("{base_topic}/pools"),
        payload,
        retain,
    ))
}

/// Concrete MQTT publisher backed by [`rumqttc::AsyncClient`].
pub struct RumqttcPublisher {
    client: AsyncClient,
    qos: QoS,
    base_topic: String,
    publish_offline_on_disconnect: bool,
    published_count: Arc<AtomicUsize>,
    acked_count: Arc<AtomicUsize>,
    ack_notify: Arc<Notify>,
    disconnecting: Arc<AtomicBool>,
    eventloop_handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl RumqttcPublisher {
    /// Connects (starts the background `rumqttc` event loop) using the given [`Config`].
    ///
    /// Also configures a Last Will and Testament (LWT) on `<base_topic>/bridge/state` with
    /// payload `"offline"` and immediately publishes `"online"` to `<base_topic>/bridge/state`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::MqttClient`] if queuing the initial availability message fails.
    pub async fn connect(config: &Config) -> Result<Self> {
        let mut mqttoptions =
            MqttOptions::new(&config.mqtt_client_id, &config.mqtt_host, config.mqtt_port);
        mqttoptions.set_keep_alive(config.mqtt_keep_alive);
        mqttoptions.set_clean_session(true);

        if let Some(ref username) = config.mqtt_username {
            let password = config.mqtt_password.as_deref().unwrap_or_default();
            mqttoptions.set_credentials(username, password);
        }

        let availability_topic = format!("{}/bridge/state", config.mqtt_base_topic);
        let last_will = LastWill::new(&availability_topic, "offline", config.mqtt_qos, true);
        mqttoptions.set_last_will(last_will);

        // Channel capacity of 256 messages to avoid backpressure stalls during burst pool publication
        let (client, mut eventloop) = AsyncClient::new(mqttoptions, 256);
        let published_count = Arc::new(AtomicUsize::new(0));
        let acked_count = Arc::new(AtomicUsize::new(0));
        let ack_notify = Arc::new(Notify::new());
        let disconnecting = Arc::new(AtomicBool::new(false));

        let acked_loop = Arc::clone(&acked_count);
        let notify_loop = Arc::clone(&ack_notify);
        let disconnecting_loop = Arc::clone(&disconnecting);
        let qos = config.mqtt_qos;

        let host = config.mqtt_host.clone();
        let port = config.mqtt_port;
        let handle = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                        info!(
                            host = %host,
                            port = port,
                            session_present = ack.session_present,
                            "Connected to MQTT broker"
                        );
                    }
                    Ok(Event::Incoming(Packet::PubAck(_))) if qos == QoS::AtLeastOnce => {
                        acked_loop.fetch_add(1, Ordering::SeqCst);
                        notify_loop.notify_waiters();
                    }
                    Ok(Event::Incoming(Packet::PubComp(_))) if qos == QoS::ExactlyOnce => {
                        acked_loop.fetch_add(1, Ordering::SeqCst);
                        notify_loop.notify_waiters();
                    }
                    Ok(Event::Incoming(packet)) => {
                        trace!(?packet, "Received MQTT packet");
                    }
                    Ok(Event::Outgoing(Outgoing::Publish(_))) if qos == QoS::AtMostOnce => {
                        acked_loop.fetch_add(1, Ordering::SeqCst);
                        notify_loop.notify_waiters();
                    }
                    Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                        debug!("MQTT client sent DISCONNECT packet; stopping event loop");
                        break;
                    }
                    Ok(Event::Outgoing(outgoing)) => {
                        trace!(?outgoing, "Sent MQTT packet");
                    }
                    Err(ConnectionError::RequestsDone) => {
                        debug!("MQTT event loop finished processing all requests and disconnected");
                        break;
                    }
                    Err(err) => {
                        if disconnecting_loop.load(Ordering::SeqCst) {
                            debug!("MQTT connection closed after disconnect request");
                            break;
                        }
                        warn!(
                            error = %err,
                            host = %host,
                            port = port,
                            "MQTT connection error; retrying in 2s"
                        );
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        });

        let publisher = Self {
            client,
            qos: config.mqtt_qos,
            base_topic: config.mqtt_base_topic.clone(),
            publish_offline_on_disconnect: !config.run_once,
            published_count,
            acked_count,
            ack_notify,
            disconnecting,
            eventloop_handle: tokio::sync::Mutex::new(Some(handle)),
        };

        // Publish online availability
        publisher
            .publish(&MqttMessage::new(availability_topic, "online", true))
            .await?;

        Ok(publisher)
    }

    async fn wait_for_pending_acks(&self) {
        let wait_fut = async {
            loop {
                let notified = self.ack_notify.notified();
                let published = self.published_count.load(Ordering::SeqCst);
                let acked = self.acked_count.load(Ordering::SeqCst);
                if acked >= published {
                    break;
                }
                notified.await;
            }
        };

        if tokio::time::timeout(Duration::from_secs(5), wait_fut)
            .await
            .is_err()
        {
            warn!(
                published = self.published_count.load(Ordering::SeqCst),
                acked = self.acked_count.load(Ordering::SeqCst),
                "Timed out waiting for all MQTT messages to be acknowledged before disconnect"
            );
        }
    }
}

impl MqttPublisher for RumqttcPublisher {
    async fn publish(&self, message: &MqttMessage) -> Result<()> {
        debug!(
            topic = %message.topic,
            retain = message.retain,
            payload_len = message.payload.len(),
            "Publishing MQTT message"
        );
        self.published_count.fetch_add(1, Ordering::SeqCst);
        if let Err(err) = self
            .client
            .publish(
                &message.topic,
                self.qos,
                message.retain,
                message.payload.as_bytes(),
            )
            .await
        {
            self.published_count.fetch_sub(1, Ordering::SeqCst);
            return Err(err.into());
        }
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        if self.publish_offline_on_disconnect {
            let availability_topic = format!("{}/bridge/state", self.base_topic);
            if let Err(err) = self
                .publish(&MqttMessage::new(availability_topic, "offline", true))
                .await
            {
                warn!(error = %err, "Failed to queue offline availability message before disconnect");
            }
        }

        // Wait until all queued publishes have been acknowledged by the broker
        self.wait_for_pending_acks().await;

        self.disconnecting.store(true, Ordering::SeqCst);

        if let Err(err) = self.client.disconnect().await {
            warn!(error = %err, "Failed to send disconnect command to MQTT client");
        }

        let handle_opt = {
            let mut guard = self.eventloop_handle.lock().await;
            guard.take()
        };

        if let Some(handle) = handle_opt {
            if tokio::time::timeout(Duration::from_secs(5), handle)
                .await
                .is_err()
            {
                error!("Timed out waiting for MQTT event loop to flush and disconnect");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_pool_messages_contains_state_and_all_subtopics() {
        let raw = r#"{
            "id": "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9",
            "title": "Piscine",
            "latestMeasure": {
                "temperature": 16.025036640276937,
                "ph": 8.351476506360877,
                "orp": 741,
                "mode": "gateway",
                "isValid": true,
                "ecoId": "0C6rTwUH",
                "gatewayCode": "I20-98cdaceb5d60",
                "measuredAt": "2026-10-23T09:08:00.000Z"
            },
            "mode": "STANDARD",
            "hasAnActionRequired": true,
            "advice": {
                "filtrationDuration": 2
            },
            "salt": null
        }"#;

        let pool: Pool = serde_json::from_str(raw).unwrap();
        let messages = build_pool_messages(&pool, "iopool2mqtt", true).unwrap();

        let find = |suffix: &str| -> Option<&str> {
            let full = format!("iopool2mqtt/cfd5a936-e8e6-4ad3-b985-512bee0d9ad9/{suffix}");
            messages
                .iter()
                .find(|m| m.topic == full)
                .map(|m| m.payload.as_str())
        };

        assert!(find("state").is_some());
        assert_eq!(find("id"), Some("cfd5a936-e8e6-4ad3-b985-512bee0d9ad9"));
        assert_eq!(find("title"), Some("Piscine"));
        assert_eq!(find("mode"), Some("STANDARD"));
        assert_eq!(find("hasAnActionRequired"), Some("true"));
        assert_eq!(find("salt"), Some("null"));
        assert_eq!(find("advice/filtrationDuration"), Some("2"));
        assert_eq!(
            find("latestMeasure/temperature"),
            Some("16.025036640276937")
        );
        assert_eq!(find("latestMeasure/ph"), Some("8.351476506360877"));
        assert_eq!(find("latestMeasure/orp"), Some("741"));
        assert_eq!(find("latestMeasure/mode"), Some("gateway"));
        assert_eq!(find("latestMeasure/isValid"), Some("true"));
        assert_eq!(find("latestMeasure/ecoId"), Some("0C6rTwUH"));
        assert_eq!(find("latestMeasure/gatewayCode"), Some("I20-98cdaceb5d60"));
        assert_eq!(
            find("latestMeasure/measuredAt"),
            Some("2026-10-23T09:08:00.000Z")
        );
    }
}
