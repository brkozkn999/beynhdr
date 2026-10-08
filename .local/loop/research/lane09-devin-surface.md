# Lane 09 — Devin CLI Yüzey Envanteri (kota/kullanım/durum verisine giden HER yol)

Tarih: 2026-10-08 · Makine: kasa (`C:\Users\pc`, Windows, Git Bash) · Yöntem: salt-okur (help/whoami/list/doctor + dosya okuma + binary strings). **Hiçbir prompt/görev başlatılmadı, token harcanmadı.**

- Binary: `devin 3000.10.27 (bcbe88c7)` — `C:\Users\pc\AppData\Local\devin\cli\bin\devin.exe` (179 MB tek exe, Rust; crate adları `chisel*`, `toolbox*`).
- Hesap: `İbrahim Berk Özkan <ibrahimberkozkan34@gmail.com>` · `user-35db4d080455414c8b55c42c2ca0b833` · Team `devin-team$account-059fe1a9bf534d1e95bbca9b85cdfc63` · Org `org-2e43493227924a80a07981da23aa79cb` · Plan **Pro** (Devin Pro) · Enterprise: hayır.
- API'ler: `https://api.devin.ai` (Devin API), `https://server.codeium.com` (inference/auth), `https://app.devin.ai` (webapp).
- Aktif süreçler: 10+ `devin.exe` (REPL + `devin acp` child'ları + bu oturum).

## 1. Komut ağacı (`devin --help` + tüm subcommand `--help`)

```
devin [OPTIONS] [PATH]... [-- PROMPT] [COMMAND]
├─ auth      login | logout | status | help
├─ mcp       add | list | get | remove | login | logout | enable | disable | help
├─ models    list (--format text|json) | help
├─ doctor    [--json]
├─ rules     list [--provider cursor|windsurf] | show | paths | help
├─ skills    list [--trigger user|model] [--json] | show | paths | help
├─ plugins   install | list | info | update | remove | prune | help
├─ cloud     drs < whoami | sandbox-create | run | blueprint-list | blueprint-create
│                  | blueprint-write | build | build-start | build-wait | build-logs
│                  | secret-create >
├─ desktop   [ARGS]...        (Devin Desktop launcher'a forward)
├─ list      [--format interactive|json|csv]   (alias: ls; cwd'ye göre filtreler)
├─ rm        <TARGET> [--force]
├─ ssh       <SESSION> [SSH_ARGS] [--gateway H]
├─ forward   <SESSION> <PORTS>... [--gateway H]
├─ update    [--force]
├─ version
├─ migrate   hooks | workflows | help
├─ sandbox   setup | help     (Research Preview)
├─ setup     [--force-manual-token-flow]
├─ uninstall [--clean] [--force]
├─ acp       [--agent-type summarizer|review] [--model M] [--refusal-fallback M]
└─ help
```

- **Top-level `usage`/`stats`/`status`/`quota`/`settings`/`sessions`/`account`/`billing`/`plan`/`limits` YOK** — denendi, hepsi `[PATH]` argümanı olarak yutulup top-level help basıyor (exit 0). Kota verisine giden tek CLI komutu `auth status` (tier/plan satırı) — detay aşağıda.
- `-p/--print` headless mod + `--export`, `-c/--continue`, `-r/--resume`, `--prompt-file`, `--config`, `--permission-mode`, `--sandbox`, `--model` flag'leri var. `--json` sadece `doctor` ve `skills list`'te; `list`/`models list` `--format json` kullanıyor.

## 2. Lokal veri dizinleri (var olanlar)

| Dizin | İçerik | Rol |
|---|---|---|
| `C:\Users\pc\.devin\` | `argv.json` (VS Code tarzı crash-reporter cfg), `extensions/extensions.json` | Devin **Desktop** (VS Code 1.126 fork) kimliği |
| `%APPDATA%\devin\` (= `Roaming\devin`) | `config.json`, `credentials.toml`, `cli\` (CLI **data**), `User\` (Desktop state), Electron cache'leri (`Cache`, `IndexedDB`, `Local Storage`, `Session Storage`, `DIPS`…) | Desktop + CLI kullanıcı verisi |
| `%APPDATA%\devin\cli\` | `sessions.db` (1.68 GB SQLite + 221 MB WAL), `logs\` (329 gz + 18 düz), `transcripts\` (55 json), `summaries\` (133 md), `session_locks\`, `plugins\`, `wiki\`, `app_state.json`, `installation_id`, `trusted_workspaces.json`, `skill_events_spool.lock` | CLI oturum deposu |
| `%LOCALAPPDATA%\devin\cli\` | `bin\devin.exe`, `_versions\{3000.10.21,3000.10.27}\{bin,share}`, `*.bin` cache dosyaları, `mcp\descriptions.json`, `cached_version.json` (`{"latest":"3000.11.3"}`), `distribution` (`irm-iex`) | CLI kurulum + **sunucu entity cache** |
| `%LOCALAPPDATA%\devin\telemetry_state.json` | `{"is_zdr":false}` | ZDR bayrağı |
| `%APPDATA%\devin\User\` | `globalStorage\state.vscdb` (152 anahtar), `acp-messages\*.db` (13 adet SQLite), `chatLanguageModels.json` | Devin **Desktop** durumu |
| Yok: `~/.config\devin`, `~/.local/share\devin`, `~/.cache\devin`, `%APPDATA%\devin\mcp_config.json`, `C:\ProgramData\Devin\system.json`, `C:\beyin\.devin`, `beynhdr\.devin` | — | Linux yolları kullanılmıyor; kurum politikası yok |

## 3. Çalıştırılan komutlar → çıktılar (kayıt)

| Komut | Çıktı | Kota verisi? |
|---|---|---|
| `devin auth status` | Text panel: credentials dosya yolu, API URL'leri, kullanıcı adı/email/ID, **Tier: "Devin Pro", Plan: "Pro"**, Team ID, membership=Approved, Enterprise=no, Telemetry=enabled | ✅ plan/tier (sayısal kota YOK) |
| `devin cloud drs whoami` | `{"org_id":"org-2e43…","devin_api_url":"https://api.devin.ai","api_key_set":true}` | ❌ |
| `devin doctor --json` | `{"ok":true,"checks":[{"check":"custom subagent profiles","status":"pass","detail":"none configured"}]}` | ❌ |
| `devin list --format json` (C:\beyin'de) | `[{id, short_id, working_directory, working_directory_display, last_activity_at(unix), last_activity_ago, title}]` | ❌ (oturum envanteri) |
| `devin models list --format json` | `{"families":[{family_label,slug,aliases,variants:[{model_uid,label,max_context_tokens,max_output_tokens,cost_tier,cost_summary,is_new,is_beta}]}]}` — örn. `swe-2-high: cost_tier="Free"`, `adaptive: cost_summary="$0.5/1M Input · $0.1/1M Cached · $2/1M Output"` | ⚠️ fiyat tarifesi (kota değil) |
| `devin rules paths` / `skills paths` | Kural/skill dizin yolları | ❌ |

## 4. Kota verisi taşıyan yüzeyler — detay

### 4.1 `%LOCALAPPDATA%\devin\cli\user_status.<digest>.bin` — **birincil kota yüzeyi**

- Format: JSON sarmal `{version:1, identity_digest, fetched_at_secs, payload:<base64 protobuf>}` — payload = `GetUserStatusResponse{user_status, plan_info}` (binary strings'te RPC adı doğrulandı). Her CLI açılışında taze dosya (`identity_digest` değişince yeni dosya; 4 snapshot duruyor: `011a9a8b…` ve `3f0de2c5…` = bugün 03:20, `321733b5…`, `65915a94…` = 15 Eyl).
- 142 KB; `user_status` = kimlik + org + feature flag'ler + model kataloğu; **`plan_info` (alan 13, 337 B) = plan/kota bloğu**.
- Decode edilmiş alanlar (proto field no → değer, iki snapshot diff'iyle doğrulandı):

| f13 altı alan | 15 Eyl snapshot | 8 Eki snapshot | Yorum (binary string adı) |
|---|---|---|---|
| `f1.2` | `"Pro"` | `"Pro"` | plan adı |
| `f1.7` | 16384 | 16384 | büyük ihtimal `acu_limit` (sabit kota) |
| `f1.8` | 600 | 600 | ikincil limit/rate |
| `f1.33` | org bloğu | aynı | `org_id`, `app.devin.ai`, `api.devin.ai`, `brkozkn999`; `f1.33.12.2`="Ask your account admin to raise it" (`request_usage_action` metni) |
| `f2.1` | 1789252361 (2026-09-12T22:32:41Z) | aynı | `plan_start` |
| `f3.1` | 1791844361 (2026-10-12T22:32:41Z) | aynı | plan dönemi sonu / `grace_period_end` (+30 gün) |
| `f8` | -1 (i64 sentinel) | -1 | unset/unlimited |
| **`f14`** | — (yok) | **100** | `daily_quota_remaining_percent` |
| **`f15`** | **26** | **48** | `weekly_quota_remaining_percent` ← değer değiştiği doğrulandı |
| **`f16`** | i64: **-279047** | i64: -279047 | `overage_balance_micros` (negatif = kredi/bakiye) |
| **`f17`** | 1789459200 (09-15T08:00Z) | 1791446400 (10-08T08:00Z) | `daily_quota_reset_at_unix` |
| **`f18`** | 1789891200 (09-20T08:00Z) | 1791705600 (10-11T08:00Z) | `weekly_quota_reset_at_unix` |

- Binary'de ayrıca `acu_consumed`, `acu_limit`, `usage_url`, `hide_daily_quota`, `hide_weekly_quota`, `billing_strategy`, `has_paid_features`, `is_devin`, `DevinPlanInfo` alan adları var (serde string'lerinden). Mapping %95 güvenle: changelog `/usage`'un "daily/weekly quota progress bars with reset times + overage balance" render ettiğini resmen doğruluyor.
- **Güvenilirlik: YÜKSEK** — sunucudan gelen resmi `GetUserStatus` payload'ı; tek eksik isim↔numara eşlemesi (inference).

### 4.2 `%APPDATA%\devin\cli\sessions.db` — oturum-bazlı kullanım

SQLite (WAL). Tablolar: `sessions`(65), `message_nodes`(111.984), `prompt_history`(504), `tool_call_state`(15.574), `app_state`, `subagent_heads`, `rendered_commits`.

- `sessions` kolonları: `id, working_directory, backend_type, model, agent_mode, created_at, last_activity_at, title, cogs_json, workspace_dirs, hidden, metadata`.
- **`sessions.metadata` JSON: `{"total_credit_cost":0, "total_acu_cost":0.0}`** — oturum başına kredi/ACU toplamı alanı var; bu makinede tüm oturumlarda 0 (SWE-2 Max / "Free" cost_tier faturalandırılmıyor olabilir — model kataloğunda `cost_tier:"Free"`).
- **`message_nodes.metadata` JSON: `num_tokens_preceding`** (mesaj başına kümülatif context token'ı; son oturumlarda max 24K–204K), `summarized_from`, `is_system_prefix`, `extensions`.
- `chat_message` JSON: `{message_id, role, content, metadata, tool_calls, tool_call_id, thinking}` — yapısal `usage{}` nesnesi YOK; `committed_acu_cost` eşleşmeleri tamamen benim grep çıktılarımın transcript'e gömülmesi (yanlış pozitif).
- **Güvenilirlik: YÜKSEK (alanlar kesin), ama kota değeri bu planda hep 0** → "kullanım hacmi" için `num_tokens_preceding` + transcripts'a bak.

### 4.3 `%APPDATA%\devin\cli\transcripts\<session>.json` — token metrikleri (55 dosya)

```json
{schema_version, session_id, agent:{name,version,model_name,tool_definitions},
 steps:[{step_id,timestamp,source,message,extra:{telemetry,generation_model},
        model_name, tool_calls, metrics:{prompt_tokens,completion_tokens,cached_tokens}, …}],
 final_metrics:{total_prompt_tokens, total_completion_tokens, total_cached_tokens, total_steps}}
```
- Örnek `dazed-waitress.json`: `final_metrics = prompt 3.424.796 / completion 55.311 / cached 3.259.266 token, 41 step`.
- `models list --format json` `cost_summary` ile çarp → oturum $ tahmini hesaplanabilir.
- **Güvenilirlik: YÜKSEK** — kalıcı, şeması stabil, per-step `metrics` gerçek API sayaçları.

### 4.4 `%APPDATA%\devin\User\acp-messages\<uuid>.db` — canlı ACP event'leri (Desktop, STALE)

13 SQLite; şema `meta(kv)` + `messages(position, kind, payload)`; kind'lar: `tool_call(313), agent_thought(116), progress_marker(104), agent_message, user_message, plan, scripted_run`. `tool_call` payload'larında `cognition.ai/eventType` sentetik event'leri:

| eventType | Adet | Kota alanı |
|---|---|---|
| `context_growth_update` | 125 | `contextGrowth{iteration_count, current_context_tokens, current_context_bytes, total_tool_output_bytes, tool_aggregates:[{tool_name,call_count,approx_ant_tokens,…}]}` |
| **`acu_consumption_at_last_user_interaction`** | 24 | **`cognition.ai/acuAmount`** — gerçek debet: 0 (reset) veya 1.465–4.59 ACU/interaksiyon |
| diğer | — | `session_analysis`, `shell_process_completed`, `rules_injected`, `devin_suspended`, `computer_use`, … |

- `meta.info` = ACP `availableCommands` listesi (slash komut reklamları).
- ⚠️ **Son yazma 15 Eyl** — Desktop oturumlarından kalma; CLI/`devin.exe` REPL bu dosyalara YAZMIYOR (CLI'nin karşılığı sessions.db + bellek-içi event stream). Şema referansı için altın; canlı kota için ölü uç.

### 4.5 `%APPDATA%\devin\User\globalStorage\state.vscdb` — Desktop mirror (STALE)

`ItemTable` (152 key). `windsurfAuthStatus` → `{"apiKey":"devin-session-token$<JWT>", userStatusProtoBinaryBase64:<GetUserStatusResponse>, allowedCommandModelConfigsProtoBinaryBase64:[…]}` — **user_status'un ikinci kopyası** + çalışan session JWT (yetki: API çağrıları). `windsurf_auth-<ad>-usages` = extension usage listesi (kota değil).

### 4.6 REPL slash komutları (interaktif — headless'tan erişilemez)

Binary string'lerinden tam liste: `/account /bug /clear /compact /context /delete /docs /doctor /editor /feedback /help /login /logout /mcp /model /new /permissions /plan /plugins /rename /resume /review /rules /session /sessions /share /skills /status /tokens /update /usage /version` + docs'tan: `/session-stats`(`/stats`), `/login-status`(ACP'te `/status`), `/org`, `/upgrade`, `/config`, `/cloud-sessions`, `/loop`, `/btw`, `/fast`, `/shortcuts`, `/hooks`, `/workspace`, `/fork`, `/continue`, `/copy`, `/feedback`, `/mouse`, `/ask`.

Bundled doc (`share/devin/docs/reference/commands.mdx` + changelog) doğrulaması:
- **`/usage`** — "estimated credit/ACU usage for the session…"; changelog: "daily/weekly quota progress bars with reset times, extra usage balance line", "quota % remaining + overage balance", "ACU usage shown".
- **`/session-stats` (`/stats`)** — "renders EVERY usage dimension the server reports — credits, ACUs, agent messages, turn continuations, token usage — using the server's own labels/grouping"; "by model" billing chart; **totals persist across resume**.
- `/context` — context window usage indicator. `/login-status` — auth. `/upgrade` — plan yükseltme. `/org` — org seçimi.
- Instant slash komutları agent çalışırken de koşuyor (queue'suz).

→ Bu komutlar `plan_info` + `session_stats` stream'ini render eder; **değerlerin kaynağı 4.1'deki `user_status.*.bin` + ACP event'leri** — yani TUI'a girmeden aynı veri dosyadan okunabilir.

### 4.7 `devin acp` (stdio JSON-RPC) — teorik canlı kaynak

ACP metodları (binary): `session/new|load|list|delete|resume|close|prompt|set_mode|set_config_option|cancel|update`, `fs/read_text_file|write_text_file`, `terminal/*`, `authenticate`. `session/*` update'leri `acu_consumption_*` + `context_growth_update` event'lerini taşır (4.4'te kanıtlı). `session/list` muhtemelen bedava ama **denemedim** (oturum açma riski). Host'lar için slash komutları da buradan reklam ediliyor.

### 4.8 Diğer dosyalar

| Yol | Format | Kota? | Not |
|---|---|---|---|
| `%APPDATA%\devin\credentials.toml` | TOML | auth | `windsurf_api_key`(189c), `api_server_url`, `devin_api_url`, `devin_webapp_host` — API token'ı burada (asla basma) |
| `%APPDATA%\devin\config.json` | JSON-C | ❌ | `devin.org_id`, `agent.model=swe-2-max`, `preferred_family_models`, `permissions.allow` |
| `%LOCALAPPDATA%\devin\cli\model_configs_v5.*.bin` | JSON{b64 proto} | ⚠️ fiyat | model kataloğu (247–886 KB), `server.codeium.com` endpoint'leri |
| `team_settings.*.bin` | JSON{b64 proto} | ❌ | org model allowlist'i |
| `unleash_definitions.*.bin` | JSON{features:json-string} | ⚠️ flag | feature flag'ler + Sentry sampling (`rate limit exceeded`, `429`, `QuotaExhausted` hata dizgileri gömülü) |
| `devin_deployment.*.bin` | JSON{b64} | ❌ | `{"api_url":"https://api.devin.ai"}` |
| `managed_plugins.*.bin` | JSON{b64} | ❌ | account/org/user id + manifest |
| `%APPDATA%\devin\cli\logs\devin_*.log[.gz]` | tracing text | ❌ | **kota telemetrisi YOK** — sadece span'lar + agent'ın kendi komutlarının ekranı (grep'lerim loglara düştü). "rate limit" eşleşmeleri hep yanlış pozitif |
| `summaries\history_*.md` (133) | Markdown dump | ❌ | tam konuşma dökümü (summarizer çıktısı) |
| `Local Storage\leveldb\*.ldb` | LevelDB | ⚠️ zayıf | `ENFORCE_QUOTA`, `PLAN_BASED`, `use-quota-for-plan…` flag anahtarları; canlı değer yok |
| `IndexedDB`, `Session Storage`, `Cache`, `DIPS` | Electron binary | ? | taramadım/okunabilir değil |
| `session_locks/*.lock` | PID text | ❌ | oturum canlılık kilidi |
| `app_state.json`, `trusted_workspaces.json`, `installation_id`, `telemetry_state.json` | JSON | ❌ | `is_zdr:false` |

### 4.9 Process/env + binary yüzey

- Env var'lar (binary strings): `DEVIN_API_URL`, `DEVIN_WEBAPP_URL`, `DEVIN_MODEL`, `DEVIN_PERMISSION_MODE`, `DEVIN_SANDBOX`, `DEVIN_REFUSAL_FALLBACK`, `DEVIN_HARNESS{,_LEAD_ONLY,_SIDEKICK_ONLY}`, `DEVIN_OUTPOST{,S}_{GATEWAY_URL,TOKEN,CONNECT_TOKEN,SESSION_ID,DESKTOP}`, `DEVIN_WORKER_*`, `DEVIN_REMOTE_SESSION_TOKEN`, `DEVIN_COMPACTION_THRESHOLDS`/`DEVIN_SIDEKICK_COMPACTION_THRESHOLDS`, `DEVIN_PREFER_EXEC_TOOL`, `DEVIN_PLUGIN_DISCOVERY`, `DEVIN_USER_EDIT_CADENCE`, `DEVIN_ID`, `DEVIN_DISABLE_HISTEXPAND`, `DEVIN_CLI_DANGEROUSLY_FOLLOW_SYMLINKS`, `WINDSURF_API_KEY` (ACP auth). Kredi/kota env'si YOK.
- Error/UI dizgileri: `RateLimited`, `QuotaExhausted`, `UsageLimitReached`, `retry_after`, `invocation_cap_reached`, `committed_quota_cost_basis_points`, `committed_overage_cost_cents`, `cognition.ai/acuUsed`, `cognition.ai/overageDollars`, `cognition.ai/statusReason` (cloud session metadata).
- Bundled docs: `…\_versions\3000.10.27\share\devin\docs\` — `reference/commands.mdx`, `models.mdx`, `changelog/stable.mdx` (**kota semantiğinin resmi kaynağı**).

## 5. beyn.stats için önerilen devin kaynak sırası

1. **`%LOCALAPPDATA%\devin\cli\user_status.*.bin`** (en yeni `fetched_at_secs`) — hesap kotası: `plan_info` → daily% (f14), weekly% (f15), overage_micros (f16), daily/weekly reset (f17/f18), plan (f1.2), dönem start/end (f2.1/f3.1). Basit proto-walker yeter; **`f14=0`/absent ayrımına dikkat** (proto3 zero omitted → absent = 0 veya yeni alan). **KOTANIN TEK GERÇEK KAYNAĞI.**
2. **`devin auth status`** — plan/tier/team/auth canlılığı (ucuz, text parse; fallback = credentials.toml + config.json).
3. **`transcripts/*.json`** — oturum-token telemetrisi: `final_metrics{total_prompt_tokens,total_completion_tokens,total_cached_tokens,total_steps}` + per-step `metrics` + `generation_model` (billed model). `sessions.db`'den bağımsız, stabil dosya başına.
4. **`sessions.db` (ro)** — `sessions.metadata.total_acu_cost/total_credit_cost` (şu an hep 0 ama alan var; credit-plan'a geçişte dolacak), `model`, `backend_type`, aktivite zamanları, `message_nodes.metadata.num_tokens_preceding` → context-baskısı serisi.
5. **`devin models list --format json`** — `cost_summary`/`cost_tier`/`max_context_tokens` ile token→$ haritalama + context doluluk %'si için payda.
6. **`%APPDATA%\devin\User\acp-messages/*.db`** (varsa/yeniyse) — `acu_consumption_at_last_user_interaction` `acuAmount` + `context_growth_update` event şeması; Desktop'tan kalma, **stale** — ama aynı event tipleri `devin acp` session/update stream'inde canlı.
7. **`devin acp` stdio** — `session/list` + event stream'iyle canlı ACU/context okuma (deneysel; önce `session/list`'in bedava olduğunu doğrula).
8. **REPL slash (`/usage`, `/session-stats`, `/context`)** — sadece insan/TUI yolu; beyn.stats için doğrudan kanal değil, ama doğrulama için aynı veriyi gösterir.
9. **Dokunma:** `logs/` (kota yok), `summaries/` (döküm), `state.vscdb` (stale mirror + JWT — key higyeni), leveldb'ler (flag çorbası), `credentials.toml` (token; sadece "var/yok" için).

## 6. Sınırlar / notlar

- `plan_info` alan↔isim eşlemesi inference (binary string sırası + snapshot diff + changelog çapraz doğrulama). Yanlış yorum riski düşük ama sıfır değil; `f15` weekly (26→48, reset f18 ile tutarlı), `f14` daily (100, reset f17 ile tutarlı).
- ACU debit değerleri Desktop acp-messages'ta var (max ~4.6 ACU/prompt); CLI oturumlarında eşdeğer event sessions.db'ye yazılmıyor — CLI tarafında ACU yalnız `user_status` (hesap seviyesi, %) + REPL render'ında yaşıyor.
- `devin -p`/`acp` session başlatmak token harcar → hiçbiri çalıştırılmadı. `devin list` cwd-scoped (`[]` /tmp'den).
- `update` mevcut: kurulu 3000.10.27, `cached_version.json` → latest **3000.11.3**.
