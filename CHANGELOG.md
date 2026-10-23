# Changelog

Toutes les modifications notables de ce projet sont documentées dans ce fichier.
Le format est basé sur [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et ce projet adhère au [Semantic Versioning](https://semver.org/lang/fr/).

## [0.1.0] - 2026-10-23

### 🚀 Nouvelles fonctionnalités
- Pont Rust asynchrone (`tokio`, `reqwest`, `rumqttc`) entre l'API publique iopool (`/v1/pools`, `/v1/pool/{id}`) et MQTT.
- Double publication MQTT : état JSON complet (`<base>/<pool_id>/state`) et sous-topics unitaires (`latestMeasure/*`, `advice/*`, etc.).
- Support natif de la découverte automatique **Home Assistant (MQTT Auto-Discovery)**.
- Mode daemon périodique (`POLL_INTERVAL_SECS`) et mode exécution unique (`RUN_ONCE=true`).
- Conteneurisation Docker multi-stage non-root et stack `docker-compose.yml` avec NanoMQ.
