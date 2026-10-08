# lane05 — beyn.hud (pane-içi ajan panosu)

**Tarih:** oturum `HERDR_SESSION=ln05`, herdr 0.9.3 (`beynhdr\target\release\herdr.exe`), Windows/PowerShell pane'leri.

## Ne yazıldı

`herdr-plugins/beyn.hud/` (tek yeni klasör; çit dışına çıkılmadı — commit/push yok, `src/` yok, global config yok):

| Dosya | İçerik |
|---|---|
| `hud.js` | `--watch` (2 sn'de `api snapshot` → ANSI clear → tablo) + `--once` (tek kare). `--snapshot <file>`, `--now <ms>`, `--cols N`, `--rows N`, `--no-color`, `--interval <ms>` test kancaları. `render()`/`buildRows()`/`clipLine()` vs. `module.exports` ile dışa açık. |
| `test/hud.test.js` | `node --test`: fixture snapshot → golden tablo (render + `--once` subprocess), token/sütun haritası, state_labels override, küçük boyutta kırpma, ANSI-orta-kesme güvenliği, `formatDuration`, snapshot-yok dayanımı — 8 test. |
| `test/fixtures/snapshot.json` | 4 pan'li sahte snapshot (devin/working, claude/blocked, codex/idle, powershell/unknown). |
| `herdr-plugin.toml` | `[[panes]]` placement=`split` → `node hud.js --watch`; `[[actions]]` hud-once. |

## Tasarım / bulgular

- Snapshot kaynağı: `herdr api snapshot` → `result.snapshot.panes[]` (ajanlı ve ajansız tüm pane'ler; `snapshot.agents[]` sadece ajanlılar, kullanılmadı). Pane içinde `HERDR_SOCKET_PATH` + `HERDR_BIN_PATH` hazır → script `HERDR_BIN_PATH || herdr` kullanıyor; `HERDR_SESSION` gerekmez.
- Sütun haritası: `display_agent → agent → terminal_title basename` (AGENT); `agent_status` + `state_labels[status]` özel metin (STATUS); `beyn_since` token'ı (epoch s/ms) varsa `now - token`, yoksa watcher'ın ilk-görüş zamanı (SÜRE); `beyn_lane` dışındaki ve `beyn_since` dışındaki tüm token'lar `k=v` (TOKENS); `beyn_lane` (LANE).
- Renk: working=yeşil, blocked=kırmızı, idle/unknown=soluk (dim), bilinmeyen özel status=sarı; `NO_COLOR`/`--no-color`/non-TTY'de kapalı.
- Boyut: `process.stdout.columns/rows` → `tput` → `COLUMNS/LINES` → 80x24. Her satır `clipLine` ile pane genişliğine kırpılır (ANSI sekansı asla yarıda kesilmez); satır sayısı `rows` ile sınırlı. `--watch` imleci gizler (`?25l`), `ESC[H` + `ESC[J` ile titreşimsiz yerinde çizer, çıkışta `?25h` geri yükler.
- **Keşif:** `report-metadata --agent <label>` guard'lıdır — pane'in `effective_agent_label` eşleşmezse title/state_labels görünmez (snapshot'ta düşer). Guard'sız rapor (`--agent` olmadan) her zaman görünür. Ayrıca aynı `--source` ile yapılan her `report-metadata`, o kaynağın önceki title/display_agent/state_labels kaydını **değiştirir** (merge değil) — p1'in "Working" etiketi bu yüzden ikinci raporda düştü ve ham `working` görünüyor.

## Doğrulama (ln05 canlı)

Kurulum: `workspace create` → `w1:p1`; 3×`pane split` → `w1:p2..p4`. Sahte durumlar:
`pane report-agent` (devin/working, claude/blocked, codex/idle) + `pane report-metadata` (title, state-label, `beyn_lane`, `quota_*`, `beyn_since` token'ları). HUD pane'de: `pane run w1:p4 'node "...\beyn.hud\hud.js" --watch'`.

`pane read w1:p4` — ilk kare (03:25:26):

```
 beyn.hud 0.1.0  03:25:26  ws:1 panes:4

PANE     AGENT    STATUS    SURE      TOKENS      LANE
---------------------------------------------------------
w1:p1    devin    working   4s        quota_used  ln05
w1:p2    claude   Blocked   1h02      quota_ctx=  ln07
w1:p3    codex    idle      4m12      -           ln09
w1:p4    powersh  unknown   4s        -           -
```

+14 sn sonra aynı pane — saat ilerledi, SÜRE büyüdü (canlı döngü kanıtı):

```
 beyn.hud 0.1.0  03:25:40  ws:1 panes:4
...
w1:p1    devin    working   18s       quota_used  ln05
w1:p2    claude   Blocked   1h02      quota_ctx=  ln07
w1:p3    codex    idle      4m26      -           ln09
w1:p4    powersh  unknown   18s       -           -
```

HUD pane'in kendisi `report-metadata --display-agent beyn.hud --token beyn_lane=hud` ile etiketlendi (son satır `beyn.hu`/`hud` — dar pane'de sütun kırpması çalışıyor). `beyn_since` token'ı p2'de `1h02`, p3'te `4m56` gibi gerçek süreler üretti; token'sız pane'ler watcher-ilk-görüş süresi gösterdi (`0s`'den büyüyor).

Not: pane ~58 sütun genişliğinde → TOKENS sütunu kırpılıyor (`quota_ctx=`); taşma yok, her satır ≤ cols.

## Test

`node --test` (`herdr-plugins/beyn.hud`): **8/8 yeşil** — golden tablo birebir eşleşiyor (fixture → `PANE AGENT STATUS SURE TOKENS LANE`).

## Bilinen sınırlar

- Çok dar pane'de TOKENS sütunu token ortasından kesilir (kelime sınırında kesme yapılmıyor).
- `--once` modunda `beyn_since` olmayan pane'ler `0s` gösterir (izleme durumu ilk karede sıfır).
- `agents[]` görünümü ve `state_change_seq` panosu dışında bırakıldı.

## Temizlik

Doğrulama sonrası `session stop ln05` + `session delete ln05` çalıştırıldı; arka plan `herdr server` kapandı.
