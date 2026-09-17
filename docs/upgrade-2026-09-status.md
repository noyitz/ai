# Praxis core upgrade 2026-09 — status, open questions, risks

Audience: Yos + Noy. As of 2026-09-16 ~18:00 UTC.
Companion docs: `upgrade-2026-09-runbook.md` (ops), `upgrade-2026-09-port-audit.md` (what was ported and why).

## TL;DR

The dogfood fork's real delta onto upstream core 0.5.5 is ported and green
locally. The isolated shadow stack in `ai-gateway-dogfood` is **live, running
the new binary, and has already metered a real streamed request end-to-end**.
Prod has not been touched — verified byte-for-byte at every step. Shadow
proving caught one **adopt-day landmine** (SSRF guard → metering would
silently stop); it is resolved in the branch. Biggest remaining risk is not
code — it's that **nobody can prove what source today's prod image was
built from** (Noy question #1 below). Canary (traffic split) has not started
and needs an explicit go.

## Where we stand

| Layer | State |
|---|---|
| Branch `upgrade/2026-09` (worktree `praxis-ai-upgrade`, tip `372bd4a6` + welcome-proof commit, **local only, not pushed**) | 19 commits: 5 filter ports + generated docs + deploy stack + `provider: auto` cherry-pick + live-config alignment + deploy fixes + runbook/status docs + welcome-page client proof script |
| Tests | filters suite 1406 pass (2 known-failing upstream `credential_inject` watcher tests on macOS, fail on pristine upstream too) |
| Lint | all upstream gates green except `lint-filter-docs` flags 4 upstream docs from local rustdoc drift (proven environmental — branch changes none of their inputs) |
| Config | `praxis.yaml` in-branch mirrors the **live** cluster config + 4 required changes: `max_scratch_bytes: 1048576`, `allow_private_endpoint: true` ×4, `provider: auto` (Noy's, cherry-picked), `reasoning_effort_map` filter entry (new, qwen-scoped — fixes the `high`→400 Benny hit; access behavior unchanged: denylists match live) |
| Shadow stack | `aigateway_shadow` db (CNPG `Database` CR), isolated metering-service-shadow, `praxis-ai-shadow` BC/IS, 2× praxis-shadow pods on `git-c674d829` (`sha256:25876b43…`), 4 `*-shadow` routes |
| Prod | untouched, 4 consecutive verified checks: 2/2 pods @ `ae83fb…` (same pods, 0 restarts), `praxis-config` rv unchanged (`7826150`), all 4 routes `alternateBackends: <none>` |

### Proven with real traffic (shadow, zero prod impact)

- Auth per listener dialect (401 unauth, 200 keyed) — `api_key_auth` →
  maas-api validate; group (`octo-eng`) visible through `model_access`
  overrides and carried all the way into the usage row.
- Model discovery (`model_catalog`) + per-group allow/deny.
- `/v1/responses` streaming (the Codex-on-Qwen flow): SSE completes with
  usage (`total_tokens: 86`), and `aigateway_shadow.usage_events` got the
  row: 62 prompt / 24 completion / 86 total, provider `qwen-flash`, 200.
  This exercises `token_count provider: auto` + scratch + `external_metering`.
- Streaming chat-completions through the Responses bridge: byte-near-identical
  between shadow (new binary) and prod (old binary) — behavioral parity, see
  finding 2.
- **Welcome-page client matrix (all 6 client/dialect combos, headless,
  `shadow` target):** `scripts/prove-welcome-clients.sh` renders the welcome
  page's own snippets against the shadow hosts and runs them: Claude Code
  (env block + gateway discovery + hosted Qwen), Codex `--profile qwen`,
  Codex `--profile pricetag` (gpt-5.3-codex via the openai route), OpenCode
  `pricetag-hosted`, Hermes qwen + gpt dialects (hermes needs the FIXED
  recipe — the page's §06 has never worked, see finding 5c–e). 6/6 replied
  correctly and **every success metered non-zero tokens** in
  `aigateway_shadow.usage_events` (prompt 10.5k–17.4k / completion 6–127;
  rows 10–55). Rerun: `TARGET=shadow ./scripts/prove-welcome-clients.sh`
  (idempotent; `CLIENTS=` narrows to a subset). It caught five real
  welcome-page bugs — finding 5.

## Findings so far

1. **SSRF guard (RESOLVED in branch, must ship at adopt).** Core 0.5.5
   added a shared address policy for filter sub-requests
   (`apis/src/callout_target.rs`): private/ClusterIP targets are refused
   unless the filter sets an opt-in. First shadow request streamed fine but
   metered **zero** — `external_metering` couldn't reach
   `http://metering-service:8080`. Fix = `allow_private_endpoint: true` on
   all four `external_metering` entries (commit `3fa269e7`), re-tested →
   row above. Old prod binary has no guard, so nothing to do today; but
   **adopt without these lines = metering silently stops**. `api_key_auth`'s
   maas-api call is not covered by the guard (verified working unchanged).
   Resolvable is an understatement — it's two config lines, already in.
2. **Chat→Responses bridge emits no client-side usage chunk (PRE-EXISTING,
   not an upgrade regression).** A `stream:true` chat-completions request
   for Qwen on the anthropic router returns SSE without a usage chunk on
   **both** prod and shadow binaries (2931 vs 2942 bytes, same shape). Noy:
   is that vLLM/emerg-side behavior you want to keep, or should the bridge
   surface `response.completed` usage to chat clients? Either way the
   upgrade doesn't change it; Codex itself uses `/v1/responses` which is
   fully metered.
3. **Config drift almost bricked the shadow boot (mitigated).** The live cm
   had been hot-reload-patched with `provider: auto` (Noy's unmerged
   branch + today's untagged image); the new binary refused it. We
   cherry-picked the feature and re-anchored the branch manifest to
   live-extracted truth. **Ongoing hazard:** the live cm is a mutable
   hand-edited object that prod hot-reloads. Any further manual patch
   (litellm migration?) silently moves prod off the branch's mirror again.
   Suggested discipline: cm changes only via the branch manifest, or at
   least announce + re-diff before adopt.
4. **CNPG db provisioning** is declarative now (`Database` CR `shadow-db`);
   teardown keeps the db by default. No action needed, noted for ops.
5. **Welcome page ships five broken client snippets** (caught by the matrix;
   page bugs, not upgrade issues). CODEX: (a) The hosted-Qwen provider is
   written `base_url = "{{UNIFIED_URL}}"` (welcome.html:422) — Codex appends
   `/responses` to the base, and the gateway serves `/v1/responses`, so the
   snippet 404s as written; the page's own "two traps" callout describes
   exactly this failure. `{{OPENAI_URL}}` already ends in `/v1`, which is why
   the pricetag profile works and the qwen one doesn't. Fix: add `/v1`
   (branch `fix/welcome-codex-qwen-base-url` in metering-service, rebased
   onto upstream `c4d2874`). (b) The snippets use the legacy `[profiles.x]`
   table, which current Codex (0.152) rejects alongside `--profile` —
   profiles now live in per-profile `<name>.config.toml` files. HERMES
   (upstream §06, `722a56f`+`c4d2874`; never worked against PriceTag as
   written, proved 2026-09-16, shadow rows 23–55): (c) **the dangerous one**
   — the `anthropic` provider ignores `model.base_url` in the yaml (its
   registry reads `ANTHROPIC_BASE_URL` from env only), so the page's qwen
   config silently falls through to hermes' fallback providers and answers
   with ZERO gateway calls: the user thinks they're onboarded, nothing is
   metered — exactly the unmetered-usage hole Q3 cares about. Fix: put
   `ANTHROPIC_BASE_URL=<unified>/v1` (+ `OPENAI_BASE_URL=<openai url>`,
   both keys) in `~/.hermes/.env`; hermes loads `.env` into its credential
   pool and the metered row proves it. (d) The `custom` provider ignores
   env keys entirely and sends a literal `no-key` placeholder bearer, so the
   GPT lineup 401s "invalid API key" with a perfectly good key in `.env` —
   `model.api_key` must be set in config.yaml. (e) The GPT default
   `gpt-5.3-codex` is responses-only on this gateway while hermes' custom
   provider speaks chat/completions → 404 "Use the v1/responses endpoint";
   `gpt-5.4` works (verified metered), `gpt-5.6-luna` 400s on hermes'
   tools+reasoning_effort combination. Also cosmetic: Claude Code's
   background `claude-haiku-4-5@20251001` calls 404 (not in the catalog) —
   pre-existing on prod too, metered as 404 rows with 0 tokens. Hermes'
   auxiliary/probe calls land as extra `model='unknown'` 0-token rows —
   metering records them, dashboards filter.

6. **Qwen `reasoning_effort` 400 — fixed with a new qwen-scoped filter.**
   Qwen3.8's vLLM chat template only accepts `xhigh/medium/low`; the
   `high` that Claude Code/Codex-style clients send (Responses bridge
   copies `reasoning.effort` verbatim) 400s — reproduced on shadow both
   dialects (rows 56–57, metered 0-token), matching what bennyturns saw
   on the emerg host. Shipping `reasoning_effort_map` (new filter,
   unified chain): rewrites chat `reasoning_effort` and Responses
   `reasoning.effort` via a configurable map (default `high→xhigh`,
   `minimal→low`), gated on the router-selected cluster with `clusters`
   mandatory — gpt-* models keep `high` (they legitimately accept it;
   gpt-5.6-luna verified). 17 unit tests; runbook §2 lists it as the
   fourth adopt-day config entry.

## Q1–Q4 — asked, answered, and what each answer still leaves open

Answers relayed by Yos (from Noy) 2026-09-16 evening.

1. **"Confirm your code includes the codex fix Noy pushed yesterday."**
   Confirmed: Noy's 9/15 push is `8b73b4db` (`token_count provider: auto`),
   cherry-picked as `5df19c4f` — all 8 of his tests pass on the branch, and
   the shadow row proves it counts live. **Still open:** the provenance of
   the prod image itself (`sha256:ae83fb…`) for the three *other* branches
   (`feat/overlay-apikey-strategy`, `feat/token-count-prompt-cache`, fork
   `fix/token-count-responses-api`) — need an explicit "none of those are
   in ae83fb" before canary.
2. **"Keep hosted models configured for Inferact/Qwen3.8-Flash-Next-NVFP4."**
   Kept: the branch's `praxis.yaml` mirrors the live catalog entry verbatim
   (~line 246), and the shadow matrix just showed all four hosted-Qwen
   clients working through it. Constraint recorded for the litellm-octo WIP:
   whatever patches the live cm next must preserve this entry.
3. **"Make sure we track all tokens spent."** Matrix result: all four
   welcome-page client paths (messages, responses×2, hosted) meter non-zero
   end-to-end. The only zero-count path is the chat-completions→Responses
   *bridge* (finding 2, pre-existing on prod, same under Noy's original
   `provider: auto` code) — fixing it is a semantics decision for Noy, not
   an upgrade blocker. Error rows (404 etc.) correctly meter 0 tokens with
   their status code.
4. **"Does the shadow day mean a different anthropic URL?"** Yes — identical
   client configs, hosts get `-shadow` appended
   (`oc -n ai-gateway-dogfood get routes | grep shadow`), or just run
   `scripts/prove-welcome-clients.sh` for the scripted version. No client
   changes beyond the hostname.

## Risks & mitigations

| # | Risk | Status / mitigation |
|---|---|---|
| 1 | Adopt stops metering silently (SSRF guard) | **Mitigated** — opt-in lines in branch cm + runbook adopt checklist; shadow row is the regression test |
| 2 | Prod image contains unmerged Noy work → adopted branch regresses features | **OPEN — top risk.** Mitigation: answer Q1, then superset-check + re-run the §4 proof suite on the merged tip before any traffic shifts |
| 3 | Live cm hand-patched again mid-flight | Mitigation: re-extract + `--validate` diff vs branch mirror immediately before canary start; discipline per finding 3 |
| 4 | Canary degrades real users | Route-weight split only (10→50→100), watch error rate/latency/shadow rows between steps; **rollback = `alternateBackends: []`, seconds, zero pod churn**; long-lived streams stay on their backend (fine) |
| 5 | Shadow stack interferes with prod | Structurally isolated: `app=praxis-shadow`/`metering-service-shadow` selectors (verified shadow pods can never join the prod service), auto-assigned route hosts (cannot steal prod hostnames), separate db (`aigateway_shadow`), shadow BC has ImageChange triggers stripped (cannot watch the prod IS). Prod IS `praxis-ai:latest` is never rebuilt from this branch |
| 6 | Responses SSE usage counting regresses on big events | `max_scratch_bytes: 1048576` explicit in config (old binary's hardcoded 1 MiB; upstream default 64 KiB would silently zero it). Covered by the shadow row + daily drive |
| 7 | Scale-down-to-0 of prod praxis (repeat of yesterday) | Standing hard rule: nobody scales/restarts prod praxis; shadow work never touches it (4 verifications this session) |
| 8 | Qwen emerg upstream itself changes under us (litellm migration) | Not an upgrade risk per se — but A/B baselines (prod vs shadow rows for same requests) should be taken close together |
| 9 | Two failing `credential_inject` watcher tests | Upstream, pre-existing on macOS, fail on pristine `origin/main`; CI (Linux) is the arbiter |
| 10 | Doc-lint drift (4 upstream filter docs) | Environmental (local rustdoc version); do NOT commit the regenerated churn; CI stays green |

## How to proceed (decision points)

1. **Noy answers Q1–Q4** → if superset check finds gaps, merge them and
   re-run the proof suite (one `shadow.sh release --local`, ~40 min,
   zero prod impact).
2. **Finish §4 proving**: llm-katan replay (zero spend), Yos+Noy day of
   daily-drive on `*-shadow` routes, compare shadow vs prod usage-event
   shapes for the same requests.
3. **GO/NOGO for canary** (explicit call by Yos): weights 10→50→100 per
   route, rollback is a one-line patch back to `alternateBackends: []`.
4. **Adopt**: prod release pointing at the branch (now with
   `allow_private_endpoint` + `max_scratch_bytes` in the prod cm), old pods
   kept unexposed a week (same insurance as `postgresql-0`), then
   `shadow.sh teardown`, dashboards re-pointed.

## Ops quick-ref

```bash
cd ~/code/redhat/praxis-ai-upgrade
./deploy/openshift/shadow.sh status | up | release --local | teardown
oc -n ai-gateway-dogfood get routes | grep shadow        # the 4 hosts
# shadow metering rows (from a probe pod, creds via secret envFrom):
#   select * from usage_events order by id desc limit 5;   -- aigateway_shadow
```
