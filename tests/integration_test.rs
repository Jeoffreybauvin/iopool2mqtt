//! Integration tests for `iopool2mqtt`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iopool2mqtt::{
    Bridge, Config, IopoolClient, MqttMessage, MqttPublisher, Result, RumqttcPublisher,
};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::watch;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REALISTIC_POOLS_JSON: &str = r#"[
  {
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
  }
]"#;

#[derive(Clone, Default)]
struct SharedMemoryPublisher {
    messages: Arc<Mutex<Vec<MqttMessage>>>,
}

impl MqttPublisher for SharedMemoryPublisher {
    async fn publish(&self, message: &MqttMessage) -> Result<()> {
        self.messages.lock().unwrap().push(message.clone());
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_end_to_end_wiremock_to_mqtt_topics() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/pools"))
        .and(header("x-api-key", "integration-test-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(REALISTIC_POOLS_JSON, "application/json"),
        )
        .mount(&mock_server)
        .await;

    let api_url = format!("{}/v1", mock_server.uri());
    let mut env = HashMap::new();
    env.insert("IOPOOL_API_KEY", "integration-test-key");
    env.insert("IOPOOL_API_URL", api_url.as_str());
    env.insert("MQTT_BASE_TOPIC", "iopool2mqtt");
    env.insert("HA_DISCOVERY_ENABLED", "true");
    env.insert("HA_DISCOVERY_PREFIX", "homeassistant");
    env.insert("RUN_ONCE", "true");

    let config = Config::from_map(&env).unwrap();
    let api_client = IopoolClient::from_config(&config).unwrap();
    let publisher = SharedMemoryPublisher::default();
    let recorded = Arc::clone(&publisher.messages);

    let mut bridge = Bridge::new(config, api_client, publisher);
    let (_tx, rx) = watch::channel(false);
    bridge.run(rx).await.unwrap();

    let published = recorded.lock().unwrap().clone();
    let by_topic: HashMap<String, String> = published
        .into_iter()
        .map(|m| (m.topic, m.payload))
        .collect();

    let pool_id = "cfd5a936-e8e6-4ad3-b985-512bee0d9ad9";
    assert!(by_topic.contains_key("iopool2mqtt/pools"));
    assert!(by_topic.contains_key(&format!("iopool2mqtt/{pool_id}/state")));
    assert_eq!(
        by_topic
            .get(&format!("iopool2mqtt/{pool_id}/title"))
            .map(String::as_str),
        Some("Piscine")
    );
    assert_eq!(
        by_topic
            .get(&format!("iopool2mqtt/{pool_id}/latestMeasure/orp"))
            .map(String::as_str),
        Some("741")
    );
    assert_eq!(
        by_topic
            .get(&format!("iopool2mqtt/{pool_id}/advice/filtrationDuration"))
            .map(String::as_str),
        Some("2")
    );
    assert!(by_topic.contains_key(&format!(
        "homeassistant/sensor/iopool_{pool_id}_temperature/config"
    )));
    assert!(by_topic.contains_key(&format!(
        "homeassistant/binary_sensor/iopool_{pool_id}_action_required/config"
    )));
}

#[tokio::test]
async fn test_end_to_end_wiremock_to_mqtt_topics_with_pool_title() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/pools"))
        .and(header("x-api-key", "integration-test-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(REALISTIC_POOLS_JSON, "application/json"),
        )
        .mount(&mock_server)
        .await;

    let api_url = format!("{}/v1", mock_server.uri());
    let mut env = HashMap::new();
    env.insert("IOPOOL_API_KEY", "integration-test-key");
    env.insert("IOPOOL_API_URL", api_url.as_str());
    env.insert("MQTT_BASE_TOPIC", "iopool2mqtt");
    env.insert("MQTT_USE_POOL_TITLE", "true");
    env.insert("HA_DISCOVERY_ENABLED", "true");
    env.insert("HA_DISCOVERY_PREFIX", "homeassistant");
    env.insert("RUN_ONCE", "true");

    let config = Config::from_map(&env).unwrap();
    let api_client = IopoolClient::from_config(&config).unwrap();
    let publisher = SharedMemoryPublisher::default();
    let recorded = Arc::clone(&publisher.messages);

    let mut bridge = Bridge::new(config, api_client, publisher);
    let (_tx, rx) = watch::channel(false);
    bridge.run(rx).await.unwrap();

    let published = recorded.lock().unwrap().clone();
    let by_topic: HashMap<String, String> = published
        .into_iter()
        .map(|m| (m.topic, m.payload))
        .collect();

    assert!(by_topic.contains_key("iopool2mqtt/pools"));
    assert!(by_topic.contains_key("iopool2mqtt/Piscine/state"));
    assert_eq!(
        by_topic
            .get("iopool2mqtt/Piscine/title")
            .map(String::as_str),
        Some("Piscine")
    );
    assert_eq!(
        by_topic
            .get("iopool2mqtt/Piscine/latestMeasure/orp")
            .map(String::as_str),
        Some("741")
    );
    assert_eq!(
        by_topic
            .get("iopool2mqtt/Piscine/advice/filtrationDuration")
            .map(String::as_str),
        Some("2")
    );
    let ha_temp = by_topic
        .get("homeassistant/sensor/iopool_Piscine_temperature/config")
        .expect("missing HA discovery config for Piscine temperature");
    assert!(ha_temp.contains("\"name\":\"Piscine Temperature\""));
    assert!(ha_temp.contains("\"state_topic\":\"iopool2mqtt/Piscine/state\""));
    assert!(
        by_topic.contains_key("homeassistant/binary_sensor/iopool_Piscine_action_required/config")
    );
}

#[tokio::test]
async fn test_end_to_end_live_mqtt_broker_if_configured() {
    let Ok(mqtt_host) = std::env::var("TEST_MQTT_HOST") else {
        // Skip when no live MQTT broker is configured for this test run
        return;
    };
    let mqtt_port = std::env::var("TEST_MQTT_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(1883);

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/pools"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(REALISTIC_POOLS_JSON, "application/json"),
        )
        .mount(&mock_server)
        .await;

    // Subscribe to the broker before running the bridge
    let mut sub_opts = MqttOptions::new("iopool2mqtt_test_subscriber", &mqtt_host, mqtt_port);
    sub_opts.set_keep_alive(Duration::from_secs(10));
    let (sub_client, mut sub_eventloop) = AsyncClient::new(sub_opts, 128);
    sub_client
        .subscribe("iopool2mqtt_e2e/#", QoS::AtLeastOnce)
        .await
        .unwrap();

    // Wait for SubAck
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::Incoming(Packet::SubAck(_))) = sub_eventloop.poll().await {
                break;
            }
        }
    })
    .await
    .expect("timed out waiting for MQTT SubAck");

    let api_url = format!("{}/v1", mock_server.uri());
    let port_str = mqtt_port.to_string();
    let mut env = HashMap::new();
    env.insert("IOPOOL_API_KEY", "e2e-key");
    env.insert("IOPOOL_API_URL", api_url.as_str());
    env.insert("MQTT_HOST", mqtt_host.as_str());
    env.insert("MQTT_PORT", port_str.as_str());
    env.insert("MQTT_CLIENT_ID", "iopool2mqtt_test_publisher");
    env.insert("MQTT_BASE_TOPIC", "iopool2mqtt_e2e");
    env.insert("HA_DISCOVERY_ENABLED", "false");
    env.insert("RUN_ONCE", "true");

    let config = Config::from_map(&env).unwrap();
    let api_client = IopoolClient::from_config(&config).unwrap();
    let publisher = RumqttcPublisher::connect(&config).await.unwrap();

    let mut bridge = Bridge::new(config, api_client, publisher);
    let (_tx, rx) = watch::channel(false);
    bridge.run(rx).await.unwrap();

    // Collect published messages from subscriber
    let mut received = HashMap::new();
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(Event::Incoming(Packet::Publish(p))) = sub_eventloop.poll().await {
                let payload = String::from_utf8_lossy(&p.payload).into_owned();
                received.insert(p.topic, payload);
                if received.contains_key(
                    "iopool2mqtt_e2e/cfd5a936-e8e6-4ad3-b985-512bee0d9ad9/latestMeasure/orp",
                ) {
                    break;
                }
            }
        }
    })
    .await;

    assert_eq!(
        received
            .get("iopool2mqtt_e2e/cfd5a936-e8e6-4ad3-b985-512bee0d9ad9/latestMeasure/orp")
            .map(String::as_str),
        Some("741")
    );

    // Also test with MQTT_USE_POOL_TITLE=true against the live broker
    env.insert("MQTT_USE_POOL_TITLE", "true");
    let config_title = Config::from_map(&env).unwrap();
    let api_client_title = IopoolClient::from_config(&config_title).unwrap();
    let publisher_title = RumqttcPublisher::connect(&config_title).await.unwrap();
    let mut bridge_title = Bridge::new(config_title, api_client_title, publisher_title);
    let (_tx2, rx2) = watch::channel(false);
    bridge_title.run(rx2).await.unwrap();

    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(Event::Incoming(Packet::Publish(p))) = sub_eventloop.poll().await {
                let payload = String::from_utf8_lossy(&p.payload).into_owned();
                received.insert(p.topic, payload);
                if received.contains_key("iopool2mqtt_e2e/Piscine/latestMeasure/orp") {
                    break;
                }
            }
        }
    })
    .await;

    assert_eq!(
        received
            .get("iopool2mqtt_e2e/Piscine/latestMeasure/orp")
            .map(String::as_str),
        Some("741")
    );
}
