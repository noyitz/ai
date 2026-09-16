# Praxis upgrade runbook — 2026-09 (dogfood → upstream core 0.5.5)

Branch: `upgrade/2026-09` (this repo). Prod: `praxis` deployment in
`ai-gateway-dogfood`, served by 4 routes. Prod stays untouched until the
canary section. Rollback after every step is named explicitly.

## Ground rules

- Prod imagestream `praxis-ai:latest` is NEVER rebuilt from this branch —
  shadow builds go to `praxis-ai-shadow`.
- Shadow stack is rendered from prod manifests by `deploy/openshift/shadow.sh`
  — never hand-copy YAML.
- No `cargo fmt --all` on stable toolchain (repo `rustfmt.toml` is nightly;
  stable reformats ~15 upstream files).

## 1. Local gates (done — evidence in git log)

- [x] 5 filters ported, per-feature commits, each compiles
- [x] `cargo test -p praxis-ai-filters` — 1396 pass; 3 known-failing upstream
      `routing::credential_inject` watcher tests confirmed failing on pristine
      `origin/main` (macOS env)
- [x] `cargo check --workspace --all-targets`
- [x] `praxis-ai --validate` against the live config (see §2) — **passes
      unmodified on core 0.5.5**, exit 0 with dummy provider keys (real keys
      come from the `provider-credentials` secret); all five ported filters
      present in `--dump` output
- [ ] upstream conformance: `cargo xtask openai-conformance` +
      `make lint` (`lint-filter-docs` covers our 5 generated docs)

## 2. Config migration

Extract live config and validate against the new binary:

```bash
python3 -c "import yaml; cm=list(yaml.safe_load_all(open('deploy/openshift/praxis.yaml')))[0]; open('/tmp/praxis-cfg.yaml','w').write(cm['data']['praxis.yaml'])"
cargo run -p praxis-ai-proxy -- --config /tmp/praxis-cfg.yaml --validate
```

Result (2026-09-16): **clean on first try, no migration needed** — the live
337-line config validates against the new binary as-is. If a future refresh
does drift, fix `deploy/openshift/praxis.yaml` (the source of truth) and
re-validate. Note: the old hardcoded 1 MiB SSE scratch bump became the
configurable `max_scratch_bytes` upstream (default 64 KiB) — decide during
shadow proving whether streaming responses need it raised in config.

## 3. Shadow stack

```bash
./deploy/openshift/shadow.sh up        # db + secret + bc/is + deploy + cm + svc + *-shadow routes
./deploy/openshift/shadow.sh release   # builds THIS branch into praxis-ai-shadow, rolls shadow only
./deploy/openshift/shadow.sh status
```

Isolation: shadow selectors are `app=praxis-shadow`; shadow metering writes
`aigateway_shadow` db (never prod `usage_events`); shadow routes are
`ai-gateway-*-shadow`. Rollback of the whole shadow: `shadow.sh teardown`.

## 4. Prove

1. llm-katan replay (zero provider spend): point test traffic at shadow routes.
2. `SMOKE_API_KEY=sk-... curl https://<unified-shadow-host>/v1/models` + real
   chat request incl. streaming (stream_usage_inject must produce non-zero
   token counts in `metering-service-shadow`'s `usage_events`).
3. Before/after repros for each ported fix (stream usage injection,
   content_normalize on vLLM, model_catalog envelopes, model_access 403s).
4. Yos + Noy daily-drive `ai-gateway-*-shadow` for a day.
5. Compare shadow vs prod `usage_events` rows for identical requests.

## 5. Canary (the only step that touches prod traffic)

OpenShift route weighted backends, per route, 10 → 50 → 100:

```bash
oc -n ai-gateway-dogfood patch route ai-gateway-unified --type merge -p '
spec:
  alternateBackends:
  - kind: Service
    name: praxis-shadow
    weight: 10'
```

Prod `praxis` keeps weight 90 → 50 → 0 automatically (primary stays). Watch
error rate + latency + shadow `usage_events` between steps.

**Rollback (any time, seconds, no pod churn):** set `alternateBackends: []`.

## 6. Adopt

At 100% shadow for a soak period:
1. `release.sh` prod: point the ORIGINAL `praxis` deployment/BC back at the
   adopted branch (or promote shadow resources by re-rendering prod from
   validated praxis.yaml).
2. Keep old pods unexposed a week (same insurance pattern as postgresql-0).
3. `shadow.sh teardown`, re-point watchers/dashboards.

## Known risks

- `credential_inject` watcher tests fail on macOS — upstream, pre-existing.
- Route weight = request-level split; long-lived streams stay on their
  original backend (fine).
- api_key_auth/model_access open question (custom filters vs upstream auth
  machinery) does NOT gate the upgrade — re-expressing them is a follow-up.
