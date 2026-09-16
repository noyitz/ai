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
- [x] `cargo test -p praxis-ai-filters` — 1406 pass (incl. 8 ported
      provider-auto tests); 2 known-failing upstream
      `routing::credential_inject` watcher tests confirmed failing on pristine
      `origin/main` (macOS env)
- [x] `cargo check --workspace --all-targets`
- [x] `praxis-ai --validate` against the live config (see §2) — **passes
      unmodified on core 0.5.5**, exit 0 with dummy provider keys (real keys
      come from the `provider-credentials` secret); all five ported filters
      present in `--dump` output
- [x] `make lint` — all gates pass (clippy ×2 feature sets, nightly
      `fmt --check`, machete, lint-deps/separators, example tests, README
      syncs, inference/responses checks) EXCEPT `lint-filter-docs`, which
      flags 4 upstream docs (`a2a`, `mcp`, `anthropic_messages_format`,
      `openai_responses_format`) from LOCAL rustdoc version drift: our branch
      changes none of their inputs (proven `git diff`-clean against the
      adopted upstream tip), so it fails identically on pristine upstream on
      this machine. Do NOT commit the regenerated churn for those 4.
- [ ] `cargo xtask openai-conformance` — needs `oasdiff` (not installed
      locally); the two new upstream commits since our base are responses-API
      internals we don't route through, so we inherit upstream CI's green.
- [x] rebased onto `origin/main` tip (`3bf887c5`, 2026-09-16) — clean replay
      of all 11 commits, full local gate re-run: filters tests, example tests
      (incl. new `stream-usage-inject` suite test), `--validate` exit 0 on
      the rebuilt binary.

## 2. Config migration

The LIVE cluster `praxis-config` cm — not the committed manifest — is the
source of truth for prod behavior, and it had drifted from both the
committed dogfood manifest and the branch's worktree. Extract + validate
against the new binary:

```bash
oc -n ai-gateway-dogfood get cm praxis-config -o jsonpath='{.data.praxis\.yaml}' > /tmp/praxis-cfg.yaml
cargo run -p praxis-ai-proxy -- --config /tmp/praxis-cfg.yaml --validate
```

Result (2026-09-16): the live config **failed at first** with
`token_count: unknown variant 'auto'`. The cm had been hot-reload-patched
(praxis serves the old config on reload failure — no crash) with Noy's
`provider: auto` per-path dialect selection, which exists only on his
branch `noyitz/fix/token-count-provider-auto` and in the running untagged
prod image. Two changes landed on this branch:

- `5df19c4f` — cherry-pick of Noy's `8b73b4db`, resolved against upstream
  0.5.5's `token_count` (kept upstream's `max_scratch_bytes` machinery);
  all 8 of his provider-auto tests pass.
- `e760daf7` — `deploy/openshift/praxis.yaml` rebuilt from the
  live-extracted config (literal-block style preserved, byte-exact
  roundtrip asserted) plus one required migration:
  `max_scratch_bytes: 1048576` under `token_count`. Upstream replaced the
  old binary's hardcoded 1 MiB SSE scratch with a configurable 64 KiB
  default; Responses-API SSE events exceed 64 KiB, so the default would
  silently zero Qwen-via-Codex token metering.

- SSRF-guard migration (caught by shadow proving, 2026-09-16): core 0.5.5
  gates filter sub-requests to non-public IPs behind an opt-in
  (`apis/src/callout_target.rs`). Real streaming request through
  `ai-gateway-unified-shadow` streamed fine (Responses SSE
  `response.completed` with usage) but wrote **zero** rows —
  `external_metering` logs `target resolved to blocked non-public
  address …; set the filter's private-target opt-in to true`. Fix:
  `allow_private_endpoint: true` on all four `external_metering` entries
  (commit `3fa269e7`); A/B against prod proved the OLD binary reports
  fine (no guard yet), so prod needed no urgent change — but ADOPTION
  does. `api_key_auth`'s maas-api validate call is not guarded (verified
  working unchanged). Post-fix the shadow wrote the expected row:
  prompt 62 / completion 24 / total 86, username+group carried end-to-end.

`--validate` now exits 0. **Open coordination item:** the running prod
image (`sha256:ae83fb76…`) has no git provenance (no `git-<sha>`
imagestream tag, no change-cause annotation), and Noy has three further
open branches (`feat/overlay-apikey-strategy`,
`feat/token-count-prompt-cache`, fork `fix/token-count-responses-api`).
Before canary/adopt, confirm none of those are in the running image — the
adopted branch must be a superset of what prod actually runs.

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
   **DONE 2026-09-16:** 401s/200s correct per listener dialect
   (`x-api-key` on anthropic/unified, `authorization` on openai — the live
   config's `token_header` choice, not a bug); `/v1/responses` stream on
   unified → non-zero row in `aigateway_shadow.usage_events` (see §2 SSRF
   item). Note: chat-completions→Responses *bridge* path (qwen on the
   anthropic router) emits no client usage chunk on EITHER binary —
   prod-parity, flagged to Noy as pre-existing, not an upgrade regression.
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
