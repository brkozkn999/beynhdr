# lane04 — `beyn.notify` watcher spike

Tarih: 2026-10-08 · Oturumlar: `ln04` + `ln04b` (named session'lar, doğrulama sonrası stop+delete edildi)
Kapsam: `agent list` poll → status diff → Windows toast + needs-you kuyruğu. **Tamamlandı: tüm geçişler + focus-susturma + daemon doğrulandı.**

## Ne yazıldı

| Dosya | İş |
|---|---|
| `herdr-plugins/beyn.notify/notify.js` | Poll döngüsü (`agent list`, default 3s) → `agent_status` diff → `working→idle`, `→blocked`, `→done` geçişlerinde olay. Toast: BurntToast varsa `New-BurntToastNotification`, yoksa `System.Windows.Forms.NotifyIcon` balloon (Win10/11). `focused:true` pane'de toast atılmaz, sadece kuyruk. `--once` / `--daemon` / `--status` modları |
| `herdr-plugins/beyn.notify/herdr-plugin.toml` | Manifest: `once` + `status` action'ları, `split` pane'te `daemon` entrypoint'i |

State dosyaları (repo dışı değil, `.local/` altında — gitignore'lu):

- `beynhdr/.local/state/needs-you.json` — kuyruk: `{version, source, updated_at, items:[{agent, pane, status, ts, summary}]}`, cap 50, atomic tmp+rename yazım
- `beynhdr/.local/state/beyn.notify-seen.json` — pane başına son status; `--once`'lar ve daemon restart'ları arasında geçiş yakalamak için

## Doğrulama kanıtları

Simülasyon: `pane report-agent <pane> --source custom:sim --agent pi --state <s> --seq <n>` ile lifecycle basıldı (canlı devin YASAK — kullanılmadı).

**Senaryo 1 — `working→idle`, odaklı pane (w1:p1, focused:true):**

```text
$ node notify.js --once      # seed: idle
poll done: 0 event(s)
$ report-agent --state working && node notify.js --once
poll done: 0 event(s)        # idle→working olay değil, doğru
$ report-agent --state idle && node notify.js --once
INFO event working->idle pane=w1:p1 agent=pi kind=finished queued=true toast=suppressed:focused
poll done: 1 event(s)
```

Focus-aware susturma çalıştı: toast atılmadı, kuyruk yazıldı.

**Senaryo 2 — `working→blocked`, odaksız pane (w2:p1, `--no-focus` workspace):**

```text
$ report-agent w2:p1 --state blocked --seq 2 && node notify.js --once --toast-wait
DEBUG toast backend: notifyicon
INFO event working->blocked pane=w2:p1 agent=pi kind=blocked queued=true toast=notifyicon
```

`--toast-wait` ile PS senkron koştu: ~4.9s (ShowBalloonTip+sleep+Dispose), **exit=0 → toast API başarılı**. BurntToast bu makinede kurulu değil (`Get-Module -ListAvailable BurntToast` boş) → NotifyIcon balloon yolu kullanıldı. Toast'un görsel olarak görünüp görünmediği Focus Assist/bildirim ayarına bağlı — API tarafı temiz döndü; standalone `BALLOON_OK` testi de ayrıca geçti.

**Senaryo 3 — `→done`:** odaksız pane'de `idle` raporu `agent_status:"done"` olarak yüzeye çıktı (Idle+!seen → done):

```text
w2:p1 ... agent_status: done   (agent list)
INFO event blocked->done pane=w2:p1 agent=pi kind=done queued=true toast=notifyicon
```

**Daemon modu:** `--daemon --interval 2500` arka planda; `w1:p1` working→idle dizisi ~2.5s içinde yakalandı:

```text
INFO beyn.notify daemon start interval=2500ms ... toast=auto
INFO event working->idle pane=w1:p1 agent=pi kind=finished queued=true toast=suppressed:focused
```

**Kuyruk içeriği** (`needs-you.json`, 5 item — summary = `agent read --lines 3`):

```json
{"agent":"pi","pane":"w1:p1","status":"idle","ts":"2026-10-08T00:32:58.344Z",
 "summary":"PS C:\\Users\\pc> echo NEEDS-YOU-MARKER-777 ⏎ NEEDS-YOU-MARKER-777 ⏎ PS C:\\Users\\pc>"}
```

Marker'lı gerçek pane çıktısı summary'ye düştü — kuyruk girişi "neden bakmam gerekiyor" bağlamını taşıyor.

## Bulgular / nüanslar

1. **`report-agent` custom label = ~1sn'de reap.** `--agent sim-agent` gibi `detect::parse_agent_label`'in tanımadığı label'lar uygulanıyor ama pane task'ı her ~1s'de `pane_shell_is_idle` kontrolü yapıyor; shell prompt'ta boştaysa `ReportedAgentShellReturned` → `clear_self_reported_agent` ile authority siliniyor (herdr bilinmeyen ajanın process'ini tanıyamadığı için "shell döndü = ajan çıktı" sayıyor). Kaynak: `src/pane.rs:383-412`, `src/terminal/state.rs:1898-1938`. **Simülasyonda bilinen label kullan** (`pi`, `claude`, `codex`…) — `custom:<isim>` source'u ile sorunsuz.
2. **`agent list` sadece `effective_agent_label` olan pane'leri listeler** — plain shell pane'i hiç görünmez; ilk `working`/`blocked` raporu pane'i "ajan pane'i" yapar.
3. **`done` = `idle` + `seen:false`** (`pane_agent_status`, `src/app/api_helpers.rs:96-107`). Odaklı pane'de `idle`, odaksızda `done` döner — watcher iki yüzeyi de kapsıyor.
4. **Poll çözünürlüğü:** interval'den hızlı state dizileri tek poll'de collapse eder (iki rapor ard arda basılıp sonra `--once` koşulunca ara `working` görülmedi). Daemon'da 2-3s interval yeterli; `--once` zincir testlerinde her state arasına bir poll koy.
5. **Toast process yaşamı:** NotifyIcon'un balonu gösterebilmesi için PS process'i ~4.5s canlı tutuluyor (ShowBalloonTip→sleep→Dispose); daemon'da detached+unref spawn — poll'u bloklamaz, `--toast-wait` ile senkron test edilebilir. Hata sessiz: log'a düşer, akış kesilmez.
6. **`HERDR_BIN_PATH` + `HERDR_SESSION` env'leri** notify.js'e olduğu gibi geçiyor — başka session'a işaret etmek `--session` bayrağı değil env ile.

## Kapanış

- Session'lar: `ln04`, `ln04b` → stop + delete edildi (`session list`'te sadece `gece-loop` kaldı).
- Commit/push yok, `src/` yok, global config yok. Artefaktlar: `herdr-plugins/beyn.notify/` + `beynhdr/.local/state/` + bu rapor.
