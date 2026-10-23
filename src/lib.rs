//! `iopool2mqtt` library crate.
//!
//! Queries the public [iopool REST API](https://help.iopool.com/fr/articles/5537423-api-publique-iopool)
//! and publishes pool water quality measurements, filtration advice, and optional Home Assistant
//! MQTT Auto-Discovery configurations to an MQTT broker.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod bridge;
pub mod config;
pub mod discovery;
pub mod error;
pub mod iopool;
pub mod models;
pub mod mqtt;

pub use bridge::Bridge;
pub use config::Config;
pub use error::{Error, Result};
pub use iopool::{IopoolApi, IopoolClient};
pub use models::{Advice, LatestMeasure, MqttMessage, Pool};
pub use mqtt::{MqttPublisher, RumqttcPublisher};
