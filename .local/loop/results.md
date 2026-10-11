# gece loop results — 2026-10-08 (oturum: Devin/Echo, kasa SCUM)

Her tur: madde → küçük adım → kontrol → commit/geri-al → 3-4 satır not.
Stop: 07:00 / 3 üst üste takılma / 20 tur ilerlemesiz.

## Tur 0 — ortam kurulumu
- Headless named session `gece-loop` çalışıyor (`HERDR_SESSION=gece-loop`, kendi socket'i).
- Workspace `w1` + root pane `w1:p1` (powershell, ConPTY) oluştu — `pane list` JSON doğruladı.
- Drive pattern: `HERDR_SESSION=gece-loop ./target/release/herdr.exe <cmd>`.
- Not: `workspace create` `--name` almıyor, `--label` + `--cwd` kullan.

## Tur 1 — baseline `just check` (deneme 1)
- KIRMIZI — sebep çevresel: bu shell'de `zig` PATH'te yok (winget Links alias'ı eski shell'de resolve olmadı).
- `cargo clippy` → ghostty-vt build.rs "zig executable not found". Kod tarafı dokunulmadı.
- Düzeltme: PATH'e zig paket dizini + ZIG env ile tekrar koşuluyor.

## Tur 2 — B kuyruğu B1-B3 (windows-verification.md'ye işlendi)
- B1 EVET: `agent start devin-1 --kind devin` + `agent prompt --wait` → working→idle, `LOOP-OK` cevabı ekranda. Bonus: devin footer'da kota `Pro · 48% remaining (resets in 3d 8h)` + `Context: 24k/262k (9%)` görünüyor — parse edilebilir.
- B2 EVET: ctrl+c ile devin çıkış → powershell'e dönüş → `agent start devin-2` respawn; scrollback'te iki oturumun çıktısı da duruyor.
- B3: `agent prompt` sınırı ≈30k char. 1k/8k/30k OK; 31k+ bracketed-paste olup input'ta SUBMIT'siz kalıyor (devin TUI Enter'ı yutuyor); ~33k+ OS argv limiti. Kural: uzun brief → dosyaya yaz.

## Tur 3 — B4-B6 + yan bulgular
- B4 EVET: `worktree create` Windows'ta `--trust-repository` olmadan çalışıyor (w4/w5 workspace + linked worktree oluştu). Default dizin `--path` verilmezse `~/.herdr/worktrees/<repo>/<branch>`.
- B5 EVET: `terminal session observe <term-id>` Windows'ta canlı NDJSON (`{"bytes":"<base64>"}`) — replay/evidence primitifi hazır. Not: `HERDR_SESSION` unutulursa default sock'a gidiyor.
- B6 HAYIR (yarış yok): `api.rs:499-536` restore_overlay_after_exit tek-thread + previous_focus/zoom snapshot + contains_key guard; unit test'ler mevcut (api.rs:2012,2226).
- Yan bulgu: `pane read` JSON vermiyor (`--format text|ansi` sadece); `pane report-metadata --source <id> --token k=v` çalışıyor, token'lar `pane get` + `api snapshot`'ta görünüyor; `agent.view.set` sadece socket API (CLI subcommand yok), schema'da token-filtre alanı var.

## Tur 4 — baseline `just check` (deneme 2)
- 3375/3375 nextest PASS + 150 unittest OK; kırık tek adım: `bun test` — bun.exe WinGet paketinde var ama PATH'e girmemiş (Links dizini boş).
- PATH'e bun+zig dizinleri eklenip deneme 3 başlatıldı (arka plan).

## Tur 5 — baseline YEŞİL
- `just check` exit=0: bun dizini + ZIG env PATH'e eklenince tam suite geçti (3375 nextest + 150 unittest + bun test + fmt/clippy).
- Tekrarlanabilir env: `PATH+=:~/.cargo/bin + <winget bun dizini>`; `ZIG=<winget zig dizini>/zig.exe`. Windows'ta WinGet Links shim'leri bu shell'de resolve olmuyor — kalıcı not.
- A ✅ B ✅ → C (beyin.stats spike) şerit c25984c5'te sürüyor.

## Tur 6 — C `beyin.stats` spike: ÇALIŞTI (şerit c25984c5)
- `herdr-plugins/beyin.stats/` → herdr-plugin.toml (id=beyin.stats, action collect, [[panes]] hud) + collect.js (Node tek-shot). `plugin link` OK, action `beyin.stats.collect` listede.
- 3 kaynak canlı: devin footer regex (`48% left`, `resets 3d 8h`), claude JSONL usage (in/out/cache), codex rollout token_count (total 3.6M, week 16%, credits 2063). 9 token → `pane report-metadata --source beyin.stats` → `pane get`+`api snapshot`'ta kanıtlı.
- Nüanslar: report-metadata sessiz başarı; token TTL'i var; devin kota banner'i değişken (parse toleranslı olmalı); plugin action invoke context'i focused pane'den alıyor → cross-pane yazım riski var.
- Rapor: `.local/loop/c-beyin-stats.md`. → F2 "plugin'le kota panosu mümkün" DOĞRULANDI.

## Tur 7 — D `beyin.brain` wikilink parser spike: TAMAM
- `.local/loop/wikilink.js` (Node, vault salt-okur) → `wikilink-graph.json`: 214 node, 130 edge, 94 unresolved.
- Resolver: stem + göreli-yol + `beyin/` prefix-strip (Obsidian semantiği). İlk sürüm 35 edge'di; yol-formatlı `[[Companion/Core]]` linkler düşüyordu — düzeltildi.
- Topoloji: hub-spoke. `Dashboard.md` 51 outbound hub; `Proje.md`'ler ikincil; `knowledge/concepts` en çok linklenen cluster. Unresolved'ların çoğu meşru: `receipts/<sha>` vault-dışı (`.state`), klasör linkleri (`Inbox/Dump/`).
- F2.5 için sonuç: parser ucuz (tek geçiş, regex), node/edge şeması plugin overlay'ine beslenebilir.

## Tur 8 — kapanış
- Test artıkları temizlendi: w4/w5/w6 workspace + 3 worktree + 3 branch (sandbox-wt*, lane-*) silindi; p*.txt + observe.out silindi.
- Kalan workspace'ler: w1 (loop, devin-2 idle — Berk kararına kadar açık), w2 (stats, C şeridinin devin'i), w3 (sandbox).
- `RAPOR.md` yazıldı: tablo + takılanlar + Berk'e 4 karar sorusu.

DURDU: kuyruk bitti (A-D 4/4 tamam, stop-şartı değil doğal bitiş) | biten: A baseline yeşil, B1-B6, C spike, D spike | kalan: Berk kararları (native fark önceliği, plugin repo'su, session kaderi, prompt düzeltmesi)

## Tur 9 — 15 şerit dalgası başlatıldı (Berk: "uyuyacağım, kendi kendine araştır+geliştir+test et")
Build (subagent_general, kendi `lnNN` session'ları):
- 10d39167 lane01 stats-v2 → herdr-plugins/beyin.stats/ (loop modu + provenance + staleness + node:test)
- df08f077 lane02 lane-runner → herdr-plugins/beyn.lane/ (brief→worktree→pane→agent→receipt; dry-run E2E)
- af2a9d45 lane03 receipt-observer → herdr-plugins/beyn.receipt/ (observe NDJSON → sha256 zincir)
- 8b0147d9 lane04 notify → herdr-plugins/beyn.notify/ (status-diff → toast + needs-you.json, focus-aware)
- 2c178438 lane05 hud → herdr-plugins/beyn.hud/ (api snapshot → pane-içi canlı tablo)
- c08cc6cd lane06 parse-lib → herdr-plugins/beyn.parse/ (fixture'lı parser lib + node:test ≥15)
- 83abc60e lane07 keys → herdr-plugins/beyn.keys/ (keybind pack + config check)
- 6e292def lane08 layouts → herdr-plugins/beyn.layouts/ (3 preset + export/apply round-trip)
Araştırma (rapor → .local/loop/research/):
- e9db5a9f lane09 devin CLI yüzeyi · eb228a8a lane10 claude veri modeli (explore) · 9b38e6fd lane11 codex modeli (explore) · 5eb34f6b lane12 plugin gap haritası (explore) · c82bfe22 lane13 upstream drift · ca798610 lane14 event/subscribe haritası (explore) · 0cb457ea lane15 attention yüzeyi
Çit herkesde aynı: commit/push yok, src/ yok, global config yok, canlı devin = sadece lane02'de max 1 prompt.

## Şerit dönüşleri
- lane13 upstream-drift ✅: fork ff-merge edilebilir (3 commit geride); ⚠️ 2 commit bizim alanlara dokunuyor (hyperlink+mouse → preview/overlay; windows process_cwd → lane). devin.toml değişmemiş, protocol hâlâ 22. Öneri: şimdi ff-merge + tetikli sync ritmi.
- lane03 receipt-observer ✅ (doğruladım: `node verify.js` CHAIN INTACT + tamper test kırıyor; observe stream `terminal.frame` objeleri dönüyor, ilk frame full snapshot)
- lane06 parse-lib ✅ (doğruladım: `node --test` 15/0; gerçek claude/codex fixture'ları + devin footer varyantları)
- lane07 keys ✅ (config check `config:ok` sıfır diagnostik; `switch_workspace`↔`switch_tab` bilinçli swap belgeli)
- lane02 lane-runner ✅ — ÖNEMLİ BUG BULGU: `agent wait --until idle` prompt-sonrası anında pre-prompt idle'a match'liyor (turn takip etmiyor) → doğru until = `done|blocked`. E2E tamam: dry-run + pane-run akış + 1 canlı devin (permission dialog'u `blocked` yapıyor → `send-keys 1` ile onay). Ek: `worktree remove` branch'i SİLMİYOR (ayrıca `git branch -D` lazım); `agent prompt` pane-id de kabul ediyor.
- lane05 hud ✅: pane-içi canlı dashboard — `hud.js --watch` pane'de koşup `api snapshot`'tan tablo çiziyor (PANE|AGENT|STATUS|SÜRE|TOKENS|LANE), `pane read` ile canlı kanıtlandı, 8/8 test. Kritik bulgu: pane'ler `HERDR_SOCKET_PATH`+`HERDR_BIN_PATH` inherit ediyor → pane içinden `herdr api snapshot` HERDR_SESSION'sız çalışıyor.
- lane08 layouts ✅: `layout.export/apply` socket-API (CLI'da yok), JSON LayoutNode ağacı; 3 preset (lane-grid 5 eş, focus+stats, pair-review) + 3/3 round-trip PASS; `plugin link`→action invoke kanıtlı.
- lane01 stats-v2 ✅: collect.js v0.2.0 — `--loop`+`--ticks`, `*_src` provenance token'ları, staleness (`quota_age_s` + per-source cache), ≤16 chunk + monoton seq + TTL, atomic state JSON. 14/14 test + ln01'de 22-token canlı kanıt.
- lane15 attention-map ✅: 18 dikkat kanalı envanteri; `notification show` delivery `off|herdr|terminal|system`; WinRT system toast'u `OpenConsole.exe` sidecar gerektiriyor (dev binary'de YOK); BurntToast yok → NotifyIcon fallback çalışıyor; öneri: `report-agent blocked` + `--state-label` + tek `notification show --sound request`.
- lane10 claude-model ✅ (explore → raporu ben yazdım): kota% diske yazılmıyor (statusline stdin'i persist değil); rate_limit olayı jsonl'de `isApiErrorMessage`+429; token'lar `message.id` dedupe şart; `.claude.json` sadece son-oturum `lastCost`; graft katmanı ekstra kaynak (bu makinede provider=codex proxy, model'ler gpt-5.6-* → kendi fiyat tablosu şart).
- lane11 codex-model ✅ (explore → raporu ben yazdım): `token_count` event'i tam kota paketi (total/last usage + rate_limits{primary{used_percent,window_minutes,resets_at}, credits{balance}, plan_type}); tuzak: dosya saatleri LOKAL, içerik UTC; `token_count`'ta turn_id/model yok; guardian/subagent rollout'ları session_index'te yok.
- lane14 event-map ✅ (explore → raporu ben yazdım): `events.subscribe` NDJSON stream (100ms drain, 512-ring, `events_lost`→resubscribe+snapshot); metadata token'ları `pane.updated`/`workspace.metadata_updated` event'iyle AKIYOR (kota panosu subscribe-only olabilir!); `pane.output_changed` ölü schema; token'lar restart'ta persist değil → reporter reconnect'te yeniden basmalı; wildcard yok (pane başına entry).
Kalan: lane04 notify, lane09 devin-surface, lane12 plugin-gaps.
- lane09 devin-surface ✅ — BÜYÜK BULGU: kota ekran-parse değil DISK'ten okunuyor: `%LOCALAPPDATA%\devin\cli\user_status.*.bin` protobuf → f14 daily%, f15 weekly%, f16 overage_micros, f17/18 reset ts, plan+acu_limit. Ayrıca transcripts/*.json final_metrics + sessions.db + `models list --format json` cost tablosu. Slash komutları REPL-only.
- lane12 plugin-gaps ✅ (explore → raporu ben yazdım): tam yetenek haritası — manifest blokları, popup'ın pane-olmadığı, socket-only metodlar, hook'excluded event'ler (metadata→daemon+subscribe şart), metadata limitleri, sidebar'ın client-local olması. Gap: preview-pane=FORK, lane=SINIRDA, ajan↔ajan=SINIRDA, kota/HUD/toast=PLUGIN.
Kalan: lane04 notify.
- lane04 notify ✅: 3s poll→status diff→toast (NotifyIcon fallback, BurntToast yok) + needs-you.json + focus-aware susturma; E2E kanıtlı. Bulgu: bilinmeyen label'lı report-agent ~1s'de reap ediliyor → simülasyonda custom: source/known label.
- TÜM 15 ŞERİT KAPANDI. Kendi spot-check'im: hud 8/8, stats 14/14, parse 15/15 node:test yeşil; receipt zinciri INTACT+tamper-kırılgan.

## Fork dalgası (2026-10-09 gece, 4 lane paralel subagent — worktree izolasyonlu)
- lane.create `befa5837`: atomik worktree+workspace+pane+agent + adım-bazlı rollback + lane.list/destroy + `herdr api call` CLI. 3415 test yeşil.
- bus `709fa655`: bus.send/list/read/ack FIFO mailbox (pane-keyed, transient) + bus_message event + plugin hook. 3422 yeşil.
- preview `c38fbbe4`: preview.open/set/close non-PTY salt-okur pane (markdown/text, layout'a katılır, restart'ta restore). 3427 yeşil. V2 gap: remote client-shell projeksiyonu boş.
- sidebar `d90c0ac5`: sidebar.provision/.clear — plugin'ler session-scoped sidebar satırı ekliyor (config'siz). 63/63+946 test.
- Entegrasyon: 4 merge (tek gerçek conflict: additive enum/handler satırları + generated schema). Schema `HERDR_UPDATE_API_SCHEMA=1` ile regen (`f0907962`). Final `just check` exit=0. Worktree+branch temizlendi.

## Rebrand dalgasi (2026-10-10) � storage dirs .herdr -> .beynhdr
- app_dir_name: herdr->beynhdr / herdr-dev->beynhdr-dev; worktree default ~/.herdr -> ~/.beynhdr; docs (en+ja) + dev/smoke script'ler + tests/ harness'leri uyumlu.
- KAPSAM DI�I (kontrat): binary/CLI 'herdr', HERDR_* env, herdr.sock / herdr-client.sock, herdr-plugin.toml, log dosya adlari, installer store (%USERPROFILE%\.herdr\packages, install.ps1 upstream'a hizali kaldi).
- Makine migrasyonu (kopya, eskiler yedek kalir): APPDATA herdr->beynhdr + herdr-dev->beynhdr-dev, LOCALAPPDATA ayni, ~/.herdr/worktrees -> ~/.beynhdr/worktrees. Debug --help dogruladi: config = %APPDATA%\beynhdr-dev\config.toml.
- just check YESIL: 3480/3480 nextest + maintenance + bun + docs-contract.
