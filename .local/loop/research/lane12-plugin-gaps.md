# Lane 12 — Herdr Plugin API: Tam Yetenek Haritası + Beynhdr Gap Analizi

Tarih: 2026-10-08 · Salt-okur kaynak incelemesi. Kaynaklar: `src/api/schema*.rs`, `src/app/api/plugins/*`, `src/app/api/{panes,workspaces,agent_view}.rs`, `src/cli/*`, `docs/next/.../{plugins,socket-api,cli-reference,configuration}.mdx`, `tests/fixtures/plugin-smoke/`, `.local/loop/c-beyin-stats.md`, `.local/herdr-api-schema.txt`.

Temel mimari: **"There is no separate plugin SDK — the entire Herdr CLI is the plugin API"** (plugins.mdx). Sandbox yok; V1 sınırı: **"Runtime action registration and native non-terminal plugin UI are not part of plugin v1"**.

## 1. Manifest şeması (`herdr-plugin.toml`)

| Alan | Zorunlu | Kural |
|---|---|---|
| `id`,`name`,`version`,`min_herdr_version` | evet | id ≤120 `[A-Za-z0-9:._-]`; semver gate |
| `description`, `platforms` (linux\|macos\|windows; boş dizi reddedilir) | hayır | |
| `[[build]]` | 0..n | yalnız GitHub `plugin install`'de; `link` çalıştırmaz |
| `[[startup]]` | 0..n | session restore + API hazırda 1 kez; `HERDR_PLUGIN_EVENT=startup` |
| `[[actions]]` | 0..n | `id,title,command` + `contexts?` `platforms?` |
| `[[events]]` | 0..n | `on`=bilinen hook event adı |
| `[[panes]]` | 0..n | `id,title,command` + `placement?`(default overlay) + `width/height`(popup) |
| `[[link_handlers]]` | 0..n | `pattern` regex + `action` aynı plugin'e referans |

**`[[overlays]]` bloğu YOK** — overlay `[[panes]] placement="overlay"`. Yerel id'lerde nokta YASAK (`plugin.id.action` ayrışması için). `command` argv; shell'siz çalışır. Item-level `platforms` top-level'ı override eder.

Action `contexts`: **`global|workspace|tab|pane|selection`** — `agent` context YOK; server invoke'da enforce etmez (client-shell filtre ipucu).

### placement semantiği
- `overlay` (default): aktif pane üstünde zoomed geçici; gerçek pane (pane.created+layout.updated emit, tüm pane API'lerine katılır); kapanınca focus+zoom restore.
- `split`/`tab`/`zoomed`: gerçek split / yeni tab / zoomed split.
- `popup`: session-modal terminal; **public pane id YOK, HERDR_PANE_ID verilmez, pane/layout/agent API'lerine katılmaz, event emit etmez**; tek adet (ikincisi `ui_busy`); komut çıkışı/`popup.close` ile kapanır.

`plugin.pane.open` parametreleri: plugin_id, entrypoint, placement override, width/height (popup), workspace_id, target_pane_id, direction, cwd (default plugin_root), focus, env (korunan HERDR_* anahtarları ayıklanır).

### Invocation context + env
`HERDR_PLUGIN_CONTEXT_JSON`: workspace_id/label/cwd, worktree{…}, tab_id, focused_pane_*, selected_text, invocation_source (`api|keybinding|startup|link_click|<event>`), correlation_id, clicked_url. `plugin.action.invoke`'un `context` paramı merge edilir → başka bağlam taklidi mümkün.
Runtime env: `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`, `HERDR_PLUGIN_ID/ROOT/CONFIG_DIR/STATE_DIR`, context JSON, `HERDR_WORKSPACE_ID/TAB_ID/PANE_ID`, `HERDR_PLUGIN_ACTION_ID`, `HERDR_PLUGIN_EVENT(_JSON)`, `HERDR_PLUGIN_ENTRYPOINT_ID`, `HERDR_PLUGIN_CLICKED_URL`.

Limitler: stdout+stderr ayrı ayrı 64KiB cap; eşzamanlı komut 32; log ring 200. Depolama: `HERDR_PLUGIN_STATE_DIR` / `CONFIG_DIR` (hash'li slug); registry global (tüm session'lar aynı plugin seti).

## 2. Çağrılabilir yüzey

Plugin'e kısıt YOK — tam `Method` enum'u (server/session/workspace/worktree/tab/pane/popup/layout/agent/events/integration/plugin + notification.show + command.invoke + client.window_title + client_shell.surface.set).

Socket-only (CLI wrapper yok, `herdr api call` escape-hatch yok): `events.subscribe`, `events.wait`, `agent.view.set/clear`, `command.invoke`, `client_shell.surface.set`, `pane.link.*`, `pane.selection/copy_*`, `server.live_handoff`, `server.ssh_agent.register` → ham NDJSON üzerinden `HERDR_SOCKET_PATH`.

### Event hooks vs subscribe
Manifest `[[events]]` yalnız `PLUGIN_HOOK_EVENT_KINDS`'e gelir — **`workspace.metadata_updated`, `pane.updated`, `pane.output_changed`, `pane.scroll_changed`, `layout.updated` hook'lanamaz** (yüksek hacim) → metadata değişimini yakalayan plugin `[[startup]]` daemon + `events.subscribe` kurar.

### `command.invoke` gerçek kapsamı
`command_id` ephemeral `cmd_<pid>-<ns>-<idx>` — yalnız kullanıcının `[[keys.command]]` binding'lerinden üretilir; genel action registry DEĞİL. Plugin action'ı için asıl yol `plugin.action.invoke`.

### `agent.view.set`
Tek global transient slot; `source` (`plugin:<id>` kurulu+enabled plugin ister, unlink'te temizlenir); `label` ≤32; filter `all|any|not|eq|in|exists` (derinlik ≤8, ≤64 node, in ≤32); alanlar `status|workspace_id|tab_id|pane_id|agent|seen|state_change_seq` + `{token:name}`; sort ≤8 alan (`workspace_order|tab_order|pane_order|attention|status|agent|seen|state_change_seq`+token). Etki: sidebar/mobile/indexed-focus projeksiyonu; restart'ta kaybolur.

### `report_metadata` limitleri
`source` ≤80 `[A-Za-z0-9:._-]`; ≤16 token/istek; ≤32 token/kaynak toplam; key `[A-Za-z0-9_-]{1,32}`; value ≤80 (kontrol karakteri ayıklanır); null=silme; `seq` monoton (bayat rapor ok ama yok sayılır); ≤32 sequenced source/resource; `ttl_ms` 1..86.4M; **restart'ta persist yok**; token değişimi → `pane.updated`/`workspace.metadata_updated` event'i (sadece socket subscriber'a); process-exit sonrası raporlar sessizce yutulur (`metadata_report_blocked_by_process_exit`).

### Sidebar köprüsü (KRİTİK SINIR)
`[ui.sidebar.agents].rows` / `[ui.sidebar.spaces].rows` içinde `$token` çözülür; ≤16 token/satır, ≤16 satır/panel, fg/bold/dim + ≤16 rules (equals|contains|starts_with|gt|lt|hide). **Sidebar layout client-local config'tir** — plugin değer verir, satır/yerleşim ekleyemez; "sıfır-config yeni sidebar bölümü" için eksik API = server-side sidebar provisioning.

### Agent otoritesi + observe
`pane.report_agent{source,agent,state∈idle|working|blocked|unknown}` (**done yok**), `report_agent_session`, `release_agent`, `clear_agent_authority`; `resume_argv` yalnız sahip reporter'dan. `agent.start` shell foreground şart, 25+ kind, name regex; `agent.prompt/wait/read/explain/rename/focus/send_keys`; `pane.read` visible|recent|recent-unwrapped|detection + text|ansi.

`terminal session observe/control` = **client socket** (API değil): NDJSON `terminal.frame{seq,encoding:"ansi",w,h,full,bytes:b64}` + `terminal.closed`; control stdin'den input/resize/scroll/mouse/release; tek controller + `--takeover`.

### `notification.show`
title ≤80, body ≤240, position, sound none|done|request. İki yol: `toast.delivery="herdr"` → server-içi tek-slot toast (dolu→`busy`, **1/s rate limit**); aksi → foreground client shell'e `SemanticNotification` (client politikası bağımsız). Reason: `shown|disabled|rate_limited|no_foreground_client|busy`.

## 3. Gap tablosu — beynhdr vizyonu

| Vizyon | Sınıf | Gerekçe |
|---|---|---|
| Sidebar kota panosu | **PLUGIN'LE-YAPILIR** | report_metadata→$token satırlar + agent.view.set token filtresi; spike kanıtlı. Sınır: satır config'i client-local (plugin değer verir, satır ekleyemez) → sıfır-config bölüm SINIRDA |
| Chrome-içi preview pane (pane-olmayan) | **FORK-GEREKİR** | Tüm plugin yüzeyleri terminal PTY; popup bile modal terminal. "Native non-terminal plugin panes outside v1". Eksik: non-terminal render yüzeyi |
| Lane lifecycle bundle | **SINIRDA** | Primitifler tam (worktree+workspace+pane+agent+layout.apply) ama atomik `lane.create`, first-class lane kimliği, rollback yok → plugin kendisi derleyip reconcile eder; atomiklik istersen FORK |
| Ajan↔ajan mesajlaşma | **SINIRDA** | prompt/send_text + read/observe ile pipe kurulur; mailbox/bus (kuyruk, teslimat garantisi, adresleme) plugin'in kendi state+subscribe işi |
| Toast/attention | **PLUGIN'LE-YAPILIR** | notification.show + report_agent blocked/done + view.set attention; sınırlar: tek slot, 1/s limit, `seen` doğrudan set edilemez (focus işaretler) |
| Pane-gömülü dashboard | **PLUGIN'LE-YAPILIR** | plugin.pane.open → terminal app (HUD lane05'te kanıtlı); hücre-render yok — dashboard "terminal app" olmak zorunda |

## Ek notlar
- Plugin'ler birbirinin action'ını `plugin.action.invoke`+context merge ile tetikleyebilir → çok-plugin orkestrasyonu.
- Popup'ta `HERDR_PANE_ID` bilinçli yok.
- `herdr api schema --json` her binary'den taze şema verir (snapshot: `.local/herdr-api-schema.txt`).
