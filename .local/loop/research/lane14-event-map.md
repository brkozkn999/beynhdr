# Lane 14 — herdr Event/Subscription Sistemi Tam Haritası

Tarih: 2026-10-08 · Salt-okur kaynak taraması (`src/api/**`, `src/app/**`, docs).

## 0. TL;DR

- Tek event kaynağı: **`EventHub`** — process-içi `Mutex<Vec<(seq, EventEnvelope)>>`, `MAX_EVENTS=512` ring. Persist DEĞİL; handoff'ta da taşınmaz → **seq her restart'ta 0**.
- Tüketim: **`events.subscribe`** (uzun-lived NDJSON stream; tek bağlantı=tek istek; ack `subscription_started`, sonra event satırları; server tarafı 100ms poll), **`events.wait`** (tek-shot; **bugün yalnız `pane_agent_status_changed` destekli**, gerisi `unsupported_event_wait_match`), `pane.wait_for_output`, `agent.wait`, `agent.prompt --wait`.
- Seq/offset yalnız server-içi; wire'da seq yok → `events_lost` → yeniden subscribe + `session.snapshot` reconcile.
- **Token/metadata event'i VAR:** `workspace.report_metadata` → `workspace.metadata_updated` (full WorkspaceInfo+tokens); `pane.report_metadata` → `pane.updated` (full PaneInfo+tokens); TTL expiry de aynı event'leri emit eder. **`pane.agent_status_changed` token TAŞIMAZ** — kota için `pane.updated`/`workspace.metadata_updated` dinle.
- `pane.output_changed`: schema'da tanımlı ama prod'da **hiç emit edilmiyor** + subscribe edilemiyor (ölü schema).

## 1. EventKind tam listesi

Kaynak: `src/api/schema/events.rs` — `EventKind` L194-221, `EventData` L422-556, `Subscription` L18-85. Wire: `EventEnvelope{event,data}`, `data` snake_case tagged.

| EventKind | dot_name | payload | subscribable |
|---|---|---|---|
| WorkspaceCreated/Updated/Closed/Renamed/Moved/Reordered/Focused | `workspace.*` | `{workspace: WorkspaceInfo}` (id/label/index varyantları) | ✅ `{}` |
| WorkspaceMetadataUpdated | `workspace.metadata_updated` | `{workspace}` tokens dahil | ✅ |
| WorktreeCreated/Opened/Removed | `worktree.*` | `{workspace, worktree: WorktreeInfo}` | ✅ |
| TabCreated/Closed/Renamed/Moved/Focused | `tab.*` | `{tab: TabInfo}` varyantları | ✅ |
| PaneCreated/Closed/Focused | `pane.*` | `{pane: PaneInfo}` / `{pane_id,workspace_id}` | ✅ |
| PaneUpdated | `pane.updated` | `{pane}` tokens dahil | ✅ |
| PaneMoved | `pane.moved` | `{previous_pane_id, previous_workspace_id, previous_tab_id, pane, …}` — **public pane_id değişebilir** | ✅ |
| PaneOutputChanged | `pane.output_changed` | — | ❌ prod'da emit yok, varyant yok |
| PaneExited | `pane.exited` | `{pane_id,workspace_id}` (respawn'dan ÖNCE) | ✅ |
| PaneAgentDetected | `pane.agent_detected` | `{pane_id,workspace_id,agent?,released,final_status?}` | ✅ |
| PaneAgentStatusChanged | `pane.agent_status_changed` | `{pane_id,workspace_id,agent_status,agent?,title?,display_agent?,state_labels}` | ✅ pane-scoped `{pane_id,agent_status?}` |
| LayoutUpdated | `layout.updated` | `{layout: PaneLayoutSnapshot}` | ✅ |

### Yalnızca-subscription (sampling) türleri — EventKind'de YOK
- `pane.output_matched{pane_id, source, lines?, match{substring|regex}, strip_ansi}` → edge-triggered (match'e girişte 1 kez)
- `pane.agent_status_changed{pane_id, agent_status?}` → status zaten eşitse ilk poll'da "initial" event
- `pane.scroll_changed{pane_id}` → scroll metrics diff

### İsimlendirme tuzakları
- Subscribe **filter** `type`: **dot** isimler; lifecycle event **`event` alanı**: **snake_case**; sampled 3 event'in `event` alanı **dot**; `events.wait` `match_event.event`: **snake_case**. İstemci iki formatı da normalize etmeli.

### AgentStatus
`idle|working|blocked|done|unknown`. Mapping: `Idle + !seen → done` (tamamlandı, görülmedi), `Idle + seen → idle`. `AgentInfo` ayrıca `state_change_seq`, `completion_seq`, `tokens`, `agent_session` taşır.

## 2. Subscribe API

### `events.subscribe` (stream)
- `{"method":"events.subscribe","params":{"subscriptions":[{"type":..., ...}]}}` → ack `subscription_started` → 100ms döngüde batch drain, event'ler NDJSON.
- Herhangi bir entry fail → TÜM request reddedilir + bağlantı kapanır.
- Cursor: internal seq; **512-entry ring aşımı → `EventHistoryError::Lost` → `{"error":{"code":"events_lost",…}}` + bağlantı kapanır** → resubscribe + snapshot.
- Wildcard YOK: pane-scoped türler `pane_id` zorunlu; tüm pane'ler = pane başına entry; pane set değişince yeni bağlantı (pane.created'da).

### `events.wait` (tek-shot)
- `match_event.event` snake_case; sadece `pane_agent_status_changed{pane_id,agent_status}` kabul; başarı `wait_matched`+event, timeout `timeout`, pane kapanırsa `pane_not_found`.

### Diğer wait'ler
- `agent.wait{target,until?,timeout_ms?}` event-driven; default until `idle|done|blocked`; agent çıkışı → `agent_not_running`. **Uyarı: `idle` prompt-sonrası pre-prompt state'e match olabiliyor (turn takip etmiyor) → `done|blocked` kullan (lane02 bulgusu).**
- `agent.prompt --wait`: submit+wait atomik; 5s içinde working/blocked yoksa `agent_prompt_stalled`.
- `pane.wait_for_output{pane_id,source,lines?,match,timeout_ms?}` → `output_matched` `{revision, matched_line, read}`.

### `session.snapshot`
`SessionSnapshot{version,protocol,focused_*,workspaces,tabs,panes,layouts,agents}` — event cursor içermez; subscribe-first→snapshot-second deseni: event'leri invalidation sinyali say, payload'ı koşulsuz replay etme.

## 3. Event üretim noktaları

`emit_event` = plugin hook'ları + `event_hub.push`; `emit_workspace_token_updated` plugin hook bypass. Özet emit map: pane.updated ← token patch + TTL expiry + agent rename + terminal title; workspace.metadata_updated ← token patch + TTL expiry; pane.agent_status_changed ← status VEYA presentation diff (`emit_pane_state_update`); pane.exited ← PaneDied (respawn öncesi); pane.agent_detected ← label değişimi/release.

## 4. Güvenilirlik

- Persist yok; restore'da event replay yok (client `session.snapshot` ile bootstrap).
- **Metadata token'ları persist DEĞİL** — restart sonrası reporter'ların yeniden `report_metadata` yapması şart (reconnect sinyali: bağlantı kopması + snapshot diff).
- Ring overflow → `events_lost`+close. Windows socket = named pipe `\\.\pipe\<path>`; `HERDR_SOCKET_PATH` override.

## 5. Bileşen öneri tablosu

| Bileşen | Dinle | Poll | Not |
|---|---|---|---|
| kota panosu (beyin.stats) | `workspace.metadata_updated` + `pane.updated` (payload'da full tokens) | `session.snapshot` bootstrap/resync | Token event'lerle akar; TTL expiry otomatik event'lenir; restart'ta yeniden report |
| lane runner | `pane.created/exited/closed/moved`, `pane.agent_detected`, `workspace.*`, `worktree.*`, pane başına `pane.agent_status_changed` | `agent.prompt --wait`, `agent.wait(done\|blocked)`, `pane.wait_for_output` | Wildcard yok → pane.created'da yeni subscribe bağlantısı; `pane.moved`'da `previous_pane_id` ile re-key |
| notify watcher | pane başına `pane.agent_status_changed` (filtreli blocked/done), `pane.agent_detected`, `pane.exited` | `agent.list` global görünüm | Event data'da title/display_agent/state_labels var → toast metni için ekstra okuma gerekmeyebilir; `done`=idle+unseen |
| receipt | `pane.exited/closed`, `pane.agent_status_changed(done)`, ops. `pane.output_matched` (marker regex) | `pane.read`, `pane.process_info`, `session.snapshot` | Event history kalıcı değil → kendi durable log'a yaz; `events_lost`'ta "recovered" işaretle |

### Ortak pattern
subscribe→ack→snapshot→cache; `events_lost`/kopma → resubscribe+snapshot+reconcile; event payload'ını cache'e koşulsuz yazma.
