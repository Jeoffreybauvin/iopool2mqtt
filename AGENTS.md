# AGENTS.md — Guide pour les Agents IA (`iopool2mqtt`)

Ce document fournit le contexte technique, les conventions d'architecture et les commandes nécessaires pour naviguer, maintenir et faire évoluer le projet **`iopool2mqtt`**.

---

## 1. Vue d'ensemble du projet

- **Objectif** : Interroger l'API REST publique d'**iopool** (`https://api.iopool.com/v1`) avec l'en-tête `x-api-key` et publier toutes les statistiques des piscines/spas sur un broker **MQTT** (à la fois en JSON complet et en sous-topics unitaires), ainsi que les messages de découverte automatique **Home Assistant (MQTT Auto-Discovery)**.
- **Langage** : Rust (édition 2021, MSRV `1.75`, `#![forbid(unsafe_code)]`).
- **Contrainte d'environnement importante** : **Rust n'est pas installé sur la machine hôte de développement.** Toutes les commandes `cargo` (`build`, `test`, `clippy`, `fmt`) doivent impérativement être lancées via **Docker** (`rust:1-bookworm` ou `docker compose`). En revanche, `curl` et `docker` sont disponibles localement.

---

## 2. Structure du dépôt

```text
iopool2mqtt/
├── .env                    # Variables d'environnement locales (ignoré par git, contient IOPOOL_API_KEY)
├── .env.example            # Modèle de fichier .env documenté et versionné
├── Cargo.toml              # Manifeste Rust, dépendances et configuration stricte des lints Clippy
├── deny.toml               # Configuration cargo-deny (audit licences et sécurité supply-chain)
├── Dockerfile              # Build multi-stage (rust:1-bookworm -> debian:bookworm-slim non-root)
├── docker-compose.yml      # Stack locale : emqx/nanomq:latest + iopool2mqtt
├── README.md               # Documentation utilisateur et opérateur complète
├── AGENTS.md               # Ce fichier de référence pour les agents IA
├── .github/workflows/
│   ├── ci.yml              # Pipeline CI : rustfmt, clippy, rustdoc, cargo test (+ NanoMQ), docker build
│   └── security.yml        # Pipeline Sécurité : cargo-audit, cargo-deny, Trivy FS/Secrets & Image scan
├── src/
│   ├── main.rs             # Point d'entrée binaire minimal (dotenvy, tracing, signaux OS, lancement Bridge)
│   ├── lib.rs              # Racine de la bibliothèque et ré-exports publics
│   ├── error.rs            # Types d'erreurs typés via `thiserror` (`Error`, `Result<T>`)
│   ├── config.rs           # Chargement et validation de la configuration (`Config::from_env`, `Config::from_map`)
│   ├── models.rs           # Modèles Serde (`Pool`, `LatestMeasure`, `Advice`, `MqttMessage`)
│   ├── iopool.rs           # Trait `IopoolApi` et client HTTP asynchrone `IopoolClient` (`reqwest` + `rustls`)
│   ├── discovery.rs        # Générateur de payloads Home Assistant MQTT Auto-Discovery
│   ├── mqtt.rs             # Trait `MqttPublisher`, `RumqttcPublisher` et construction des topics par piscine
│   └── bridge.rs           # Orchestrateur `Bridge<A, M>` (mode daemon avec `tokio::sync::watch` ou `run_once`)
└── tests/
    └── integration_test.rs # Tests d'intégration de bout en bout (wiremock + publisher mémoire & broker MQTT live)
```

---

## 3. Détails de l'API Publique iopool

Documentation officielle : <https://help.iopool.com/fr/articles/5537423-api-publique-iopool>

- **Header d'authentification** : `x-api-key: <IOPOOL_API_KEY>`
- **Endpoints** :
  - `GET https://api.iopool.com/v1/pools` : Retourne un tableau JSON de tous les bassins (`Vec<Pool>`).
  - `GET https://api.iopool.com/v1/pool/{id}` : Retourne l'objet JSON d'un bassin spécifique (`Pool`).
- **Exemple de réponse réelle vérifiée** :
  ```json
  [
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
  ]
  ```
- **Résilience Serde** : Les structures `Pool`, `LatestMeasure` et `Advice` dans `src/models.rs` utilisent `#[serde(default)]` sur les champs optionnels ainsi que `#[serde(flatten)] pub extra: BTreeMap<String, Value>` afin de ne jamais échouer si un bassin neuf n'a pas encore de mesure ou si iopool ajoute de nouveaux champs à son API. Tout champ supplémentaire capturé dans `extra` est automatiquement publié dans son sous-topic MQTT correspondant.

---

## 4. Conventions de code et Bonnes Pratiques Rust

Le projet suit les règles des skills locaux (`.agents/skills/rust-skills` et `.agents/skills/rust-async-patterns`) :

1. **Sécurité & Zéro `unsafe`** : `#![forbid(unsafe_code)]` est activé dans `Cargo.toml`.
2. **Gestion des erreurs** :
   - Bibliothèque et binaire (`src/*.rs`) : utiliser `crate::error::{Error, Result}` basé sur `thiserror`.
   - Aucun `.unwrap()` ou `.expect()` dans le code de production (seulement autorisé dans les modules `#[cfg(test)]`).
3. **Observabilité & Secrets** :
   - Utiliser `tracing` (`info!`, `warn!`, `error!`, `debug!`, `#[instrument]`) avec des champs structurés.
   - Ne jamais afficher `IOPOOL_API_KEY` ni `MQTT_PASSWORD` dans les logs (`Config` implémente manuellement `fmt::Debug` avec `"[REDACTED]"`).
4. **Testabilité par injection de dépendances** :
   - `Bridge<A: IopoolApi, M: MqttPublisher>` dépend d'abstractions (traits), ce qui permet de tester toute la logique métier avec des mocks (`wiremock` ou implémentations en mémoire) sans dépendre d'Internet.
5. **Arrêt gracieux (`RumqttcPublisher`)** :
   - `RumqttcPublisher::disconnect` publie `offline` sur `<base_topic>/bridge/state`, appelle `client.disconnect().await`, puis attend la fin de la tâche `EventLoop` (qui s'arrête sur `ConnectionError::RequestsDone` une fois tous les paquets en attente envoyés au broker). Cela garantit qu'en mode `RUN_ONCE=true`, aucun message n'est perdu avant la fermeture du processus.

---

## 5. Commandes de référence (via Docker)

### Lancer formatage, clippy et tests unitaires/intégration
```bash
docker run --rm -v "$PWD":/app -w /app rust:1-bookworm \
  sh -c "rustup component add rustfmt clippy && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-targets --all-features"
```

### Démarrer NanoMQ et tester `iopool2mqtt` en conditions réelles
```bash
# 1. Démarrer NanoMQ
docker compose up -d nanomq

# 2. Exécuter un cycle unique d'iopool2mqtt vers NanoMQ
RUN_ONCE=true docker compose run --rm iopool2mqtt

# 3. Vérifier les messages retenus sur NanoMQ
docker exec nanomq nanomq_cli sub -h 127.0.0.1 -p 1883 -t "iopool2mqtt/#" -v
```
