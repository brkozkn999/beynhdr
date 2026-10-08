# Lane 15 — "Dikkat Yüzeyi" Haritası

Tarih: 2026-10 (repo `beynhdr`, binary `target/release/herdr.exe` v0.9.3 stable).
Doğrulama: `HERDR_SESSION=ln15` headless server ile canlı probe; session sonunda stop+delete yapıldı.

## 1. Kanal Envanteri

### 1.1 Komut/API kanalları (kullanıcıya ulaşan)

| # | Kanal | API / komut | Görünürlük | Kalıcılık | Focus-bağımlı mı? |
|---|-------|-------------|------------|-----------|-------------------|
| 1 | **Semantic agent notification** (NeedsAttention / Finished / UpdateInstalled) | `herdr pane report-agent <pane> --source ID --agent L --state blocked|idle|working|unknown` → state geçişi tetikler; server `ServerMessage::SemanticNotification` broadcast'ler (tüm shell client'lara) | TUI içi kart VEYA outer terminal OSC VEYA OS toast — client'ın `ui.toast.delivery` seçimine göre | TUI kartı: NeedsAttention 8sn, Finished 5sn, Update 3sn; OS toast: Action Center'da kalıcı; state'in kendisi sidebar'da kalıcı | Evet — hedef pane aktif tab'da + outer terminal focus'taysa suppress (bkz §3) |
| 2 | **Custom notification** | `herdr notification show <title> [--body T] [--position TL/TR/BL/BR] [--sound none|done|request]` → API `notification.show` | Aynı delivery politikasına tabi (herdr/terminal/system); kind=Custom | TUI kartı 5sn; kuyruk max 8 | Hayır — target alanı yok, her zaman gösterilir (delivery≠off ise). Server tarafı 1sn rate-limit |
| 3 | **Outer window title** | `herdr terminal title set <title>` / `clear` → `client.window_title.set/clear`; ayrıca config `ui.window_title` template | TUI dışı — OS pencere başlığı, görev çubuğu, alt-tab, WT sekmesi | Kalıcı (clear'a kadar; clear template'e döner) | Hayır |
| 4 | **Terminal bell (BEL passthrough)** | Pane PTY'si `\x07` → `AppEvent::TerminalBell` → foreground client'a `ServerMessage::TerminalBell` → stdout'a N×BEL | TUI dışı — outer terminal'in kendi bell davranışı (WT: ses/taskbar flash) | Anlık | Hayır (sadece foreground client'a gider; focus kontrolü yok) |
| 5 | **Ses (mp3)** | Otomatik: state geçişinde `Sound::Done`/`Request`. Manuel: `notification show --sound`. Config `[ui.sound]` | TUI dışı — sistem sesi | Anlık | Kısmen — `Finished`+Done sesi hedef aktif+focus'ta suppress; `Blocked`+Request sesi **suppress edilmez** |
| 6 | **Pane metadata (display-only)** | `herdr pane report-metadata <p> --source ID [--title T] [--display-agent A] [--state-label STATUS=TEXT] [--token K=V] [--ttl-ms N] [--seq N]` | TUI içi — sidebar agent satırları (state_icon, state_text, agent, terminal_title, $token) | Kalıcı (TTL ile otomatik expire opsiyonel) | Hayır |
| 7 | **Workspace metadata** | `herdr workspace report-metadata <ws> --source ID --token K=V` | TUI içi — sidebar space satırları `$name` token'ı | Kalıcı | Hayır |
| 8 | **Sidebar status göstergeleri** | Otomatik: agent_status (idle/working/blocked/done/unknown) → `status_icon`+`status_color`; workspace satırı `aggregate_state` | TUI içi — sidebar her satırda renkli işaret | State sürdükçe kalıcı | Hayır |
| 9 | **In-TUI toast kartı** | `ui.toast.delivery="herdr"` iken semantic+custom event'ler | TUI içi köşe kartı (config `[ui.toast.herdr] position`, default bottom-right); tıkla → pane focus | Süreli (kind'a göre 3-8sn) | Hedef aktifse herdr kartında gösterilmez (Terminal/System'da suppress_external) |
| 10 | **Terminal (OSC) toast** | `ui.toast.delivery="terminal"` → `terminal_notify::show_notification` → OSC 9 (Ghostty/iTerm2/WezTerm) veya OSC 99 (Kitty) | TUI dışı — terminal emulator'ün kendi bildirimi | Terminal'e bağlı | `target_active && outer_focused` ise suppress |
| 11 | **System (OS) toast** | `ui.toast.delivery="system"` → `platform::show_desktop_notification` | TUI dışı — OS bildirim merkezi; **Windows'ta tıklanabilir** (pane'e focus'lar) | Action Center'da kalıcı | Aynı suppress kuralı |
| 12 | **Copy feedback toast** | OSC 52 clipboard veya copy-mode `y` → `CopyFeedback` kartı | TUI içi köşe (config `[ui.toast.clipboard] enabled/position`, default bottom-center) | Kısa süreli | Hayır |
| 13 | **COPY mode bar** | Copy mode'a girince mod barı: ` COPY ` rozeti + search `n/m` sayacı + keybind ipuçları | TUI içi (pane alanının üst/alt satırı) | Mode aktifken | Hayır |
| 14 | **Pane scrollbar** | `ui.pane_scrollbars=true` (default) — scrollback varsa pane kenarında thumb | TUI içi | Scrollback varken kalıcı | Hayır |
| 15 | **Global menu badge** | `update_available` veya `integration_updates_available` → menü noktası + "update ready" item | TUI içi | Update uygulanana dek | Hayır |
| 16 | **Tab bar sağ segmentleri** | `ui.tab_bar_right` config: `zoom|hostname|datetime|text|command` (command'ı server 5sn'de bir çalıştırır); `accent` boyaması | TUI içi tab satırı | Config'e bağlı | Hayır |
| 17 | **Mobil banner** | Dar terminalde (mobile layout) alt satır banner: `● title · body` | TUI içi | Süreli | — |
| 18 | **Endpoint/diagnostic banner'ları** | `endpoint_notices`, `machine_diagnostics`, config diagnostic satırları (sarı) | TUI içi | Koşul sürdükçe | Hayır |

### 1.2 State → kanal eşlemesi (server tarafı, `src/app/actions.rs`)

- `Working|Blocked → Idle` = background completion → **Finished** toast + `Done` sesi.
- `* → Blocked` = needs attention → **NeedsAttention** toast + `Request` sesi.
- `update.suppress_completion` (report-agent'daki) iki kanalı da keser.
- `pane.seen=false` (Idle iken görülmemiş) → API'de `AgentStatus::Done` projeksiyonu; sidebar'da "done".

## 2. Bildirim Pipeline'ı (önemli detaylar)

- **`notification show`** (`src/cli/notification.rs` → `handle_notification_show_api`, `src/server/headless/notifications.rs:222`): bir **client shell (TUI)** bağlı değilse `shown:false`. Reason: `disabled` (delivery=off iken) veya `no_foreground_client`. Bağlıysa broadcast `SemanticNotification{kind:Custom}` — title 80, body 240 char'a sanitize; rate limit 1/bildirim/sn (`API_NOTIFICATION_RATE_LIMIT`, `src/app/api.rs:19`).
- **İki paralel yol var**: legacy `Notify{kind:Sound|Toast|SystemToast}` sadece *foreground client'a* (`send_notify_to_foreground_client`) — non-shell eski client yolu (`handle_notify`). Modern yol `SemanticNotification` → tüm shell client'lara; sunum kararı client'ta (`client/shell/notification_policy.rs`).
- **Suppression modeli** (`active_tab_suppresses_notifications`): pane aktif tab'da **ve** outer terminal focused (veya focus bilinmiyor) → toast+Done sesi suppress. `outer_focused` client'ta DECSET 1004 focus in/out ile izlenir; `None` = bilinmiyor = "focused sayılır" (muhafazakâr).
- **Client-side doğrulama**: Finished yalnızca projeksiyon `Done` iken emit edilir (1sn grace + 50ms recheck); NeedsAttention yalnızca `Blocked` iken. Stale event'ler düşer.
- **Kuyruk**: aynı anda 1 görünür kart + max 8 queued; aynı pane'ın yeni event'i eskisini ezer.
- **Ses varken delivery=off**: `notification show --sound request` → delivery=off olsa bile ses çalar (sound effect, delivery match'ten önce push'lanır). Görsel kart düşer.

## 3. Windows toast gerçekliği

### Native herdr `system` delivery (`src/platform/windows/notifications.rs`)

- WinRT `ToastNotificationManager`, AppUserModelId `Herdr.Desktop`, XML `ToastGeneric`, `<audio silent="true"/>` (toast kendi sesini çıkarmaz — ses mp3 ile ayrı kanaldan).
- Tıklama: `activationType="protocol"` + per-exe URL scheme `herdr-notification-<sha256(exe-path)>` → `conpty/x64/OpenConsole.exe --headless -- herdr.exe --notification-activate <uri>` → gizli pencereye mesaj → `NotificationActivated` → endpoint+workspace+pane focus (`notification_target_is_current` boot_id+pane_id doğrular).
- Registry'ye yazar: `HKCU\Software\Classes\AppUserModelId\Herdr.Desktop` (DisplayName + `CustomActivator` `{D58C72D3-...}`) + scheme kayıtları.
- **Kritik bağımlılık**: `register()` exe'nin yanında `conpty/x64/OpenConsole.exe` ister — yoksa `NotFound` hatası, toast hiç çıkmaz. **Bu repo'daki `target/release/` altında `conpty/` YOK** — dev binary'de `delivery="system"` sessizce log'a warn düşer. Packaged install (`distribution/install.ps1:389-430`) `conpty/conpty.dll + x64/arm64/OpenConsole.exe` paketler — installed binary'de çalışır.

### PowerShell gerçekliği (tek denemeler, ln15 dışı değil ama host'a dokunur — config yazılmadı)

- `Get-Module -ListAvailable BurntToast` → **kurulu değil** (`BURNTTOAST_NOT_INSTALLED`).
- `System.Windows.Forms.NotifyIcon` balloon → **hatasız çalıştı** (`BALLOON_SHOWN_NO_ERROR`): `NotifyIcon` + `SystemIcons::Information` + `ShowBalloonTip(3000)` exception'sız döndü. Görsel doğrulama yapılamadı (headless); Win10+'da balloon'lar toast'a map'lenir ve NotifyIcon visible kaldığı sürece çalışır. Hata vermemesi kanalın API seviyesinde açık olduğunu gösterir.
- Alternatif kalmadı: herdr'ın kendi `system` delivery'si (packaged binary'de) en doğru yol — tıklama → pane focus dahil.

## 4. "Needs-you" tasarım önerisi

Amaç: maksimum görünürlük, minimum spam. Önerilen kombinasyon (sıralı, her biri opsiyonel katman):

1. **`pane report-agent --state blocked`** (birincil sinyal — her şeyi tetikler):
   - Sidebar'da kırmızı `●`/`×` + "blocked" (veya custom label) — **kalıcı**, TUI açıkken her zaman görünür.
   - Semantic `NeedsAttention` → kullanıcının kendi `ui.toast.delivery` tercihine saygılı toast (herdr kartı 8sn / OSC / system toast) + `Request` sesi.
   - Focus-suppression bedava: kullanıcı zaten o taba bakıyorsa spam olmaz (toast düşer, Request sesi çalar).
   - Workspace aggregate'i de `blocked` gösterir — başka workspace'teyken sidebar space satırında görünür.

2. **`pane report-metadata --state-label blocked="needs you: <kısa sebep>" --title "<bağlam>"`** (kalıcı kanıt):
   - Sidebar'da "blocked" yerine kendi metnin görünür; `title` pane bağlamı.
   - `--ttl-ms` ile kendini temizleyen etiket (örn. 15dk) — stale "needs you" kalmaz.
   - Spam riski sıfır (sessiz, kalıcı, TUI içi).

3. **`notification show "<agent> needs you" --body "<ws · tab>" --sound request`** (ephemeral push — sadece anlık olayda):
   - Kind=Custom → suppression YOK, her koşulda gösterilir → **gerçek "şimdi bak" anları için**, state geçişinin üstüne değil. Aksi hâlde double-fire olur (state geçişi zaten toast üretir).
   - `--sound request` delivery=off'ta bile ses üretir → sesi garanti kanal olarak kullan.
   - Rate limit 1/sn → batch'leme gerekiyorsa birleştir.

4. **Window title — son çare, counter pattern**: `terminal title set "● herdr: N blocked"` yalnızca **ilk** blocked geçişinde set et, son çözülmede `clear`. Per-event kullanma: (a) foreground client'a globaldir, (b) `ui.window_title` template'ini clear'a kadar ezer, (c) focus'tan bağımsız hep orada → bakmakta olan kullanıcıya da kirletir. Daha iyi alternatif: `ui.window_title="{hostname}: {workspace}"` zaten workspace adını taşıyor; workspace metadata/title ile dolaylı bilgi taşınabilir.

5. **Kaçınılması gerekenler**: pane BEL yazdırmak (her okuma \x07 → WT flash sesli spam; focus'a bakmaz, her pane'den toplanır); `terminal` delivery güvenme (Windows Terminal'de `detect_backend()` None → no-op — WT kullanıcısına `delivery=terminal` hiçbir şey göstermez).

**Özet reçete**: `report-agent blocked` (tetik) + `report-metadata state-label` (kalıcı işaret) + gerekiyorsa tek `notification show --sound request` (push). Bu üçü de herdr'ın kendi suppression/policy'sine gömülü → spam'i client config'i dengeler, sender tarafı basit kalır.

## 5. Canlı doğrulama kaydı (ln15)

```
herdr server (HERDR_SESSION=ln15, arka plan) → socket ...\sessions\ln15\herdr.sock
workspace create --label ln15-test --focus → w1 / w1:p1
notification show "ln15 test" --body ... --sound request
  → {"reason":"disabled","shown":false}            # delivery=off + client shell yok
terminal title set "ln15-title"
  → {"changed":false,"reason":"no_foreground_client"}
terminal title clear → aynı
pane report-metadata w1:p1 --source ln15 --title "ln15 probe"
     --display-agent "probe-agent" --state-label blocked="needs you"
     --state-label working="grind" --ttl-ms 60000
  → pane get: display_agent/state_labels/title snapshot'a işlendi
pane report-agent w1:p1 --source ln15 --agent devin --state working → blocked
  → agent list: agent_status=blocked, workspace list: aggregate blocked
agent wait w1:p1 --until blocked --timeout 3000 → anında döndü (blocked zaten)
session stop ln15 + session delete ln15 → temizlendi; başka session'lara dokunulmadı
```

Not: TUI client takılamadığı için kart/toast görselliği doğrulanamadı; server tarafı reason'ları ve API kabul yolları doğrulandı, sunum katmanı kaynak koddan belgelendi.

## 6. Ana kaynak referansları

- CLI: `src/cli/notification.rs`, `src/cli.rs:686-718` (terminal title), `src/cli/pane.rs:1481-1706` (report-metadata), `src/cli/spec.rs:688-745`
- Server: `src/server/headless/notifications.rs` (forwarding+suppression), `src/server/notifications.rs` (kind→NotifyKind map), `src/server/headless.rs:1462-1541` (window title sync + api override)
- Client: `src/client/shell/notification_policy.rs` (durations/queue/validation/suppress), `src/client/shell/notifications.rs` (kart render), `src/client/notifications.rs` (effect dispatch + Windows queue)
- Platform: `src/platform/windows/notifications.rs` (WinRT toast), `src/terminal_notify.rs` (OSC 9/99), `src/terminal_effects.rs` (bell + OSC 0), `src/sound.rs` (mp3 → powershell MediaPlayer)
- Config: `src/config/model.rs:198-217,937-1010` (`[ui.toast]`, `[ui.sound]`, `ui.window_title`, `pane_scrollbars`, `status_indicators`, `tab_bar_right`), `src/config/sound.rs`, `src/config/window_title.rs`, `src/config/tab_bar.rs`, `src/config/sidebar.rs`
- State/UI: `src/app/actions.rs:22-147` (geçiş→toast/ses), `src/workspace/aggregate.rs` (öncelik: blocked>unseen-idle>working>idle), `src/pane/state.rs` (`seen`), `src/client/shell.rs:178-237` (ikon/renk), `src/client/shell/agent_sidebar.rs:275-312` (state_label/agent_label önceliği), `src/ui/status.rs` (copy feedback), `src/client/shell/render.rs:31-197` (mode bar/COPY)
