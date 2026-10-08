# Lane 10 — Claude Code veri modeli envanteri (kasa SCUM)

Tarih: 2026-10-08 · Salt-okur inceleme; secret değerleri taşınmadı.

## Özet

Bu makinede Claude Code 2.1.226–2.1.236 + **graft** (`@nanonets/graft` — FleetView daemon'u + hook'lar + statusline) var. Trafik Anthropic OAuth'a değil, **`provider codex`** proxy'sine gidiyor; model adları `gpt-5.6-sol` / `gpt-5.6-luna` / `gpt-5.6-terra`. `.credentials.json` içinde `claudeAiOauth{accessToken, refreshToken, expiresAt, scopes[], subscriptionType:"pro", rateLimitTier:"default_claude_ai"}` — **token değerleri boş string** (auth proxy üzerinden).

## 1. Session JSONL event şeması

Konum: `C:\Users\pc\.claude\projects\<encoded-cwd>\<sessionId>.jsonl`. Klasör adı = cwd encode (`:` ve `\` → `-`/`--`).

**Top-level `type` değerleri:**

| type | Ek alanlar / şema |
|---|---|
| `user` | `parentUuid`, `isSidechain`, `promptId`, `message{role,content}`, `uuid`, `timestamp` (ISO 8601), `permissionMode`, `origin{kind:"human"}`, `promptSource`("typed"/"sdk"), `cwd`, `sessionId`, `version`, `gitBranch`, `sessionKind`("bg"), `slug`, `isMeta`, `toolUseResult` |
| `assistant` | `message{id:"resp_*", role, model, usage{…}, content[{thinking\|text\|tool_use}], stop_reason}`, `uuid`, `timestamp`, `effort`, `session_id` (fork origin), `sessionId`, `cwd`, `gitBranch`, `slug`, `attributionSkill/Plugin/Agent` |
| `system` | `subtype`: `informational`, `turn_duration`(`durationMs`,`messageCount`,`pendingBackgroundAgentCount`), `compact_boundary`(`compactMetadata{trigger,preTokens,postTokens,cumulativeDroppedTokens,durationMs,preservedSegment}`), `stop_hook_summary`, `local_command` |
| `attachment` | `attachment.type`: `hook_success`, `hook_additional_context`, `agent_listing_delta`, `mcp_instructions_delta`, `skill_listing`, `file`, `goal_status` |
| `queue-operation` | `operation:"enqueue"|"dequeue"`, `timestamp`, `sessionId`, `content` (task-notification XML: `<task-id>`,`<output-file>`,`<status>`,`<summary>`) |
| `file-history-snapshot` | `messageId`, `snapshot{trackedFileBackups{path→{backupFileName,version,backupTime,realParentDir}}}`, `isSnapshotUpdate` |
| `last-prompt` | `leafUuid`, `sessionId` |
| `ai-title` / `agent-name` | `aiTitle` / `agentName`, `sessionId` |
| `mode` / `permission-mode` | `mode:"normal"` / `permissionMode:"auto"|"bypassPermissions"` |

**YOK:** `type:"summary"`, `type:"usage"`, `type:"error"`, `type:"progress"` (compact `system/compact_boundary` ile kaydediliyor).

**`message.usage` alanları** (assistant event): `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens_details{thinking_tokens}`, `server_tool_use{web_search_requests,web_fetch_requests}`, `service_tier`, `cache_creation{ephemeral_1h, ephemeral_5m}`, `inference_geo`, `iterations[]`, `speed`.

**Kritik:** bir API response birden çok `assistant` satırı üretir (thinking/text/tool_use ayrı) — hepsi aynı `message.id`. Token toplarken **message.id dedupe şart**.

**Timestamp:** event'lerde ISO 8601; `history.jsonl`/`.claude.json`/`roster.json`'da epoch-ms.

**Subagent'lar:** `<sessionId>\subagents\agent-<agentId>.jsonl` — `isSidechain:true` + `agentId` + `attributionAgent`; `sessionId` parent'a işaret. Yanında `agent-<id>.meta.json` → `{agentType, description, toolUseId, spawnDepth, model}`; `tool-results/*.txt` büyük tool çıktıları. Agent tool_result içinde `<usage>subagent_tokens/tool_uses/duration_ms</usage>` metni.

## 2. Rate-limit / quota

- **5h/weekly kota yüzdesi/reset diske yazılmıyor** — `rate_limit`(kalıcı alan), `quota`, `weekly` alanı hiçbir dosyada yok.
- Rate-limit **olayı** jsonl'de: `type:"assistant"` + `isApiErrorMessage:true`, `error:"rate_limit"`, `apiErrorStatus:429`, `model:"<synthetic>"`, usage sıfır, `content[0].text` = hata metni (`"All credentials for model gpt-5.6-sol are cooling down via provider codex"`).
- `/compact` gibi local command hataları `system subtype:"local_command"` içinde `<local-command-stderr>`.
- Daemon katmanı: `.claude\jobs\<short>\state.json` → `state:"blocked"`, `detail`, `needs`; `timeline.jsonl` `{at,state,detail,text}` geçişleri.
- `.claude.json` `oauthAccount` → `organizationType:"claude_pro"`, `organizationRateLimitTier:"default_claude_ai"`, `billingType:"apple_subscription"`, `hasExtraUsageEnabled:false` — tier var, canlı kota yok.
- Statusline: `settings.json` `statusLine` → graft statusline; stdin JSON'unda `session_id`, `cwd`, `agent.name`, `context_window.used_percentage` geliyor (graft statusline.js okuyor). Vendor şemasında ayrıca `model.display_name` ve `cost.*` var ama **stdin'e gelir, diske yazılmaz**.

## 3. Session → cost

- jsonl'de event başına `costUSD` yok — fiyat tablosu lazım.
- `.claude.json` (home dizininde, `.claude\` altında değil) → `projects[<cwd>]` her proje için **sadece son oturum** agregası: `lastCost`, `lastAPIDuration`, `lastDuration`, `lastStartTime`(ms), `lastLinesAdded/Removed`, token toplamları, `lastModelUsage{<model>:{…,costUSD}}`, `lastSessionId`, `lastSessionMetrics{*}`, `lastFpsAverage/Low1Pct`. Dikkat: `costUSD` gpt-5.6-* adlarıyla bile dolu → CC iç fiyat tablosu bu isimleri tanıyor.
- graft `price.js`: `input + 1.25×cacheCreate + 0.1×cacheRead` × liste fiyatı — ama tablo sadece `claude-*` regex tanır, `gpt-5.6-*` → null. **beyn.stats kendi tablosunu yazmalı.**

## 4. Lane/pane eşleme kimlikleri

- Her event'te: `sessionId` (uuid = dosya adı), `cwd`, `gitBranch`, `slug` (insan-okur ad), `sessionKind`, `entrypoint`, `version`, `uuid`/`parentUuid` zinciri, `session_id` (fork kaynağı — resume/fork lineage).
- `history.jsonl` (global): `{display, pastedContents, timestamp(ms), project(cwd), sessionId}`.
- Daemon: `.claude\daemon\roster.json` `workers[<short>]` → `{pid, sessionId, cwd, cliVersion, startedAt, dispatch{launch{sessionId→jsonl path}, env{ANTHROPIC_MODEL…}, seed{intent,name}, cols,rows}, …}`; `.claude\jobs\<short>\state.json` → `{state, detail, tempo, inFlight, fan[], tokens, linkScanPath→jsonl, sessionId, resumeSessionId, cwd, name, intent, providerEnv, createdAt, updatedAt, needs, backend:"daemon"}`; `timeline.jsonl`; `daemon.status.json`; `attach-journal/*.json`.
- `.claude.json` `projects[<cwd>].lastSessionId` → cwd → son session.

## beyn.stats'in okuyabilecekleri

1. **Oturum envanteri:** dir taraması → sessionId, cwd, slug/aiTitle, kind (bg/interaktif/sdk), version, gitBranch, ilk/son timestamp, mesaj sayısı.
2. **Token muhasebesi:** assistant event'ler `message.id` dedupe ile → model başına input/output/cache create/cache read; `compact_boundary` context düşüşleri; `turn_duration` tur süreleri; subagent breakdown.
3. **Rate-limit teşhis:** `isApiErrorMessage`/`apiErrorStatus` sayımı + zamanı; `jobs/*/state.json` blocked/needs; `timeline.jsonl`.
4. **Canlı sağlık:** `roster.json` (pid, startedAt) + `state.json` (state/tempo/fan) + jsonl mtime.
5. **Maliyet:** token × kendi fiyat tablosu (`.claude.json lastModelUsage` ile kalibre); son-oturum toplamı için doğrudan `lastCost`.
6. **Prompt geçmişi:** `history.jsonl`, `paste-cache/<hash>.txt`.
7. **Yan veriler:** `file-history/` yedekleri, `security/*` (touched_paths/baseline_sha), `projects/<p>/memory/*.md`, `.claude.json` pluginUsage/skillUsage/oauthAccount.
8. **graft katmanı:** `<repo>/graft/.cache/session/<id>.json` (graftReads/savedTokens/inputCostMicros/inputTokensBilled), `~/.graft/telemetry-queue.ndjson`.

## Eksikler / dikkat

- **Kota % (5h/weekly) ve reset zamanı diskte yok** — statusline stdin'i dışında canlı kota okunamaz; statusline payload'ı persist edilmiyor.
- Event başına USD yok → fiyat tablosu şart; `gpt-5.6-*` için Anthropic liste fiyatları geçersiz, `.claude.json costUSD`'den geri-hesap ya da proxy fiyat bilgisi lazım.
- `.claude.json` sadece **son** oturumu tutar → tarihsel toplam jsonl taramasıyla.
- `session_id` vs `sessionId` ayrımı fork/resume'da karışık — lineage'de ikisini de izle.
- Token/secret bulunan dosyalar: `.credentials.json` (şu an boş), `daemon\control.key`, `pipe.key`, `roster.json rvAuth/ptyAuth`.
- `Temp\claude\...\tasks\*.output` mevcut değil (temizlenmiş) — okuma best-effort olmalı.
- Session dosyası olmayan cwd'ler olabilir.
