## 📋 Description des changements

<!-- Décrivez brièvement ce que fait cette Pull Request -->

## 🏷️ Type de changement (Conventional Commits)

- [ ] `feat:` Nouvelle fonctionnalité (incrémente la version **MINEURE** `0.X.0`)
- [ ] `fix:` Correction de bug (incrémente la version **PATCH** `0.0.X`)
- [ ] `sec:` / `perf:` / `refactor:` / `docs:` / `chore:` (incrémente la version **PATCH** `0.0.X`)
- [ ] `feat!:` / `BREAKING CHANGE` Changement cassant (incrémente la version **MAJEURE** `X.0.0`)

*(Optionnel : vous pouvez aussi modifier manuellement `version = "X.Y.Z"` dans `Cargo.toml` si vous souhaitez imposer un numéro de version précis lors du merge sur `master`.)*

## ✅ Checklist avant merge

- [ ] `cargo fmt --all -- --check` passe sans erreur
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passe sans avertissement
- [ ] `cargo test --all-targets --all-features` passe avec succès
- [ ] Les scans de sécurité (`cargo-audit`, `cargo-deny`, `Trivy`) sont au vert
