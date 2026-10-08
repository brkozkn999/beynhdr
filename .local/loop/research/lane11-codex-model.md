# Lane 11 — Codex (Desktop) `.codex` veri modeli envanteri

Tarih: 2026-10-08 · Salt-okur; `auth.json`/`sandbox_users.json` içeriği okunmadı.

**Kaynak:** `C:\Users\pc\.codex\` — Codex **Desktop** app (originator `"Codex Desktop"`, `source:"vscode"`), `cli_version: 0.146.0-alpha.9.2`.

## 0. Dizin envanteri

| Yol | Tür | İçerik |
|---|---|---|
| `sessions\YYYY\MM\DD\rollout-*.jsonl` | JSONL | Oturum kayıtları (3 dosya, hepsi `2026\08\09`) |
| `session_index.jsonl` | JSONL | `{"id","thread_name","updated_at"}` — sadece kullanıcı thread'leri |
| `config.toml` | TOML | Kullanıcı config (§5) |
| `auth.json` | JSON | **VAR — okunmadı** (OAuth/token deposu) |
| `goals_1.sqlite`, `memories_1.sqlite`, `state_5.sqlite`, `logs_2.sqlite` (+`-shm`/`-wal`) | SQLite | İç durum DB'leri (binary) |
| `.codex-global-state.json` | JSON | Electron UI state — kota/rate-limit key'i yok |
| `.sandbox\*.log` | text | Windows sandbox setup satırları |
| `.sandbox-bin\*.exe` | bin | Sandbox runner |
| `plugins\cache`, `.tmp`, `vendor_imports\skills-curated-cache.json` | — | cache |
| `log\` | — | **YOK** — loglar `logs_2.sqlite`'ta |

## 1. Rollout zarf şeması

Her satır: `{"timestamp":"<RFC3339 ms UTC>","type":"<tag>","payload":{...}}`

Gözlenen `type`: `session_meta` (1×/dosya), `turn_context` (1×/turn: cwd, workspace_roots, model, effort, sandbox/approval), `world_state` (~1×/oturum full:true: environments, timezone, roots, skills, permissions hash), `response_item` (ham Responses-API itemları: message/reasoning/custom_tool_call(+output)/function_call(+output)), `event_msg` (UI/protokol eventleri).

### `session_meta` payload (satır 1)
```jsonc
{"session_id":"…",        // thread id — SUBAGENT'TE PARENT'IN id'si!
 "id":"…",                // bu rollout'un id'si = dosya adındaki UUID
 "parent_thread_id":"…",  // sadece subagent'ta
 "timestamp":"2026-08-09T12:06:13.062Z",
 "cwd":"C:\\Users\\pc\\Documents\\Codex\\…",
 "originator":"Codex Desktop",
 "cli_version":"0.146.0-alpha.9.2",
 "source":"vscode",       // subagent'ta OBJECT: {"subagent":{"other":"guardian"}}
 "thread_source":"user",  // | "subagent"
 "model_provider":"openai",
 "base_instructions":{…}, "dynamic_tools":[…], "history_mode":{…}}
```
**`model` alanı session_meta'da YOK** → `turn_context.model`/`thread_settings_applied`'tan oku.

### `event_msg` payload tipleri (gözlenen)
`task_started{turn_id,started_at(unix sn),model_context_window(258400),collaboration_mode_kind}`, `task_complete{turn_id,last_agent_message,started_at,completed_at,duration_ms,time_to_first_token_ms}`, `turn_aborted{reason:"interrupted",duration_ms}`, `user_message{client_id,message,images,…}`, `agent_message{message,phase:"commentary"|"final_answer",memory_citation}`, `agent_reasoning{text}`, **`token_count`** (§2), `web_search_end{call_id,query,action,results[]}`, `patch_apply_end{call_id,turn_id,stdout,stderr,success,changes{path:{type}}}`, `thread_settings_applied{thread_settings:{model,provider,service_tier,approval_policy,permission_profile,cwd,effort,reasoning_summary,personality,collaboration_mode}}`.

Desktop farkı: shell/web/patch çağrıları `custom_tool_call` (`name:"exec"`, input=JS: `tools.shell_command(…)` vb.) + `function_call` (`name:"wait"` REPL polling). UI event'i olarak sadece `patch_apply_end`/`web_search_end` yansıyor.

## 2. `token_count` event şeması (kota kaynağı)

```jsonc
{"type":"event_msg","payload":{"type":"token_count",
 "info":{"total_token_usage":{"input_tokens":20154,"cached_input_tokens":11008,
   "cache_write_input_tokens":0,"output_tokens":396,"reasoning_output_tokens":200,
   "total_tokens":20550},
  "last_token_usage":{…aynı şema (delta)…}, "model_context_window":258400},
 "rate_limits":{"limit_id":"codex",
  "primary":{"used_percent":12.0,"window_minutes":10080,"resets_at":1786848630},
  "secondary":null,
  "credits":{"has_credits":true,"unlimited":false,"balance":"2062.5172350000"},
  "plan_type":"plus","rate_limit_reached_type":null,"spend_control_reached":null}}}
```
- `resets_at` unix saniye; `window_minutes:10080` = 7 gün (bu hesapta primary = haftalık).
- `credits.balance` string decimal.
- `token_count`'ta `turn_id`/`model` yok → turn atfı satır sırasıyla; `used_percent` float.

## 3. Zaman/kimlik/model

- Zarf `timestamp` + `session_meta.timestamp`: RFC3339 UTC ms; `session_index.updated_at` nanosaniye; **`sessions\YYYY\MM\DD` klasörü + dosya adı LOKAL saat** (UTC+3 İstanbul).
- `task_*` zamanları unix saniye.
- Model: `turn_context.model`, `thread_settings_applied.thread_settings.model`, `effort`/`reasoning_effort` — dosya içinde değişebilir.

## 4. Rollout → pane/oturum eşleşmesi

- Anahtar: `session_meta.id` = dosya UUID; `session_index.jsonl` id→thread_name.
- `session_meta.cwd`/`turn_context.cwd`/`world_state.environments.local.cwd` → pane cwd.
- **Tuzak:** guardian/auto-review rollout'ları `thread_source:"subagent"` + `source:{"subagent":{…}}` — `session_id` parent thread'i gösterir (≠ `id`); `session_index`'te yok → pane görünümünden dışla ya da parent'a attribute et.

## 5. `config.toml` kota/gözlem anahtarları

Kota/gözlem ayarı **yok**. Tek ilgili: `[desktop] show-context-window-usage = true`. Diğer key'ler: `notify`, `approval_policy`, `sandbox_mode`, `model`, `model_reasoning_effort`, `[features]js_repl`, `[mcp_servers.*]`, `[shell_environment_policy.set]`, `[windows]sandbox`, `[projects.'…']trust_level`.

## 6. `beyn.stats` okuyabilecekleri

| İhtiyaç | Kaynak |
|---|---|
| Token kullanımı (input/cached/cache_write/output/reasoning, toplam+delta) | `token_count.info.total_token_usage` / `last_token_usage` |
| Context penceresi + doluluk | `info.model_context_window` + `total_tokens` |
| Kota | `token_count.rate_limits.primary{used_percent,window_minutes,resets_at}`; `secondary` (null-tolerant); `plan_type`; `credits{balance}`; `rate_limit_reached_type`, `spend_control_reached` |
| Oturum listesi+isim | `session_index.jsonl` |
| Oturum→proje/pane | `session_meta.{id,session_id,parent_thread_id,cwd,thread_source,source}` |
| Model+effort per turn | `turn_context` / `thread_settings_applied` |
| Turn süresi/TTFT | `task_started`/`task_complete`/`turn_aborted` |
| Araç kullanımı | `response_item.custom_tool_call.name`, `patch_apply_end`, `web_search_end` |
| Tarih filtreleme | `sessions\YYYY\MM\DD` (lokal) + satır `timestamp` (UTC) |

## 7. Eksikler

- SQLite DB'ler (`logs_2`/`state_5`/`goals_1`/`memories_1`) binary — `logs_2` muhtemel log kaynağı; şema keşfi için sqlite okuması gerekir.
- Tek günlük örneklem; `secondary` non-null, `rate_limit_reached_type` non-null, compaction/undo/approval event'leri gözlenmedi — handle-edilebilir bırak.
- Upstream referans: `codex-rs/protocol/src/protocol.rs` (`SessionMeta`), `codex-rs/rollout/src/recorder.rs`.
