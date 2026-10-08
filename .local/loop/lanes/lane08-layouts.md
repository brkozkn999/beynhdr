# lane08 — `beyn.layouts`: lane-odaklı layout preset'leri

**Sonuç:** 3 preset üretildi, `layout.apply` ile ln08'de uygulandı, `pane layout`/`pane list` ile geometri kanıtlandı, export→apply round-trip üç preset için de PASS (aynı rect'ler + aynı normalize ağaç). Plugin manifest `plugin link`/`action invoke` ile uçtan uca doğrulandı, sonra unlink edildi (global iz yok).

## 1. Layout komut/API yüzeyi

- CLI'da `layout export|apply` **yok**; `herdr api` altında sadece `snapshot|schema`. Layout metodları **socket API**'sinde:
  - `layout.export` `{tab_id?|pane_id?}` → `{workspace_id, tab_id, zoomed, focused_pane_id, root}`. `root` = recursive `LayoutNode`.
  - `layout.apply` `{root, tab_id?|workspace_id?, tab_label?, focus?}` → ağaçtan **yeni tab** kurar (`tab_id` verilirse o tab'i replace eder); son layout'u döner. Limitler: ≤24 pane, derinlik ≤16. Leaf'teki `pane_id` apply'da yok sayılır (taze id üretilir); `label`→manual_label, `cwd`/`command`/`env` pane spawn'ında kullanılır.
  - `layout.set_split_ratio` `{path:[bool…], ratio, tab_id?|pane_id?}` → tek split'i yeniden oranlar; `path=[]` root split, `false`=first, `true`=second.
- `LayoutNode`: `{type:"pane", label?, cwd?, command?, env?, pane_id?}` | `{type:"split", direction:"right"|"down", ratio, first, second}`.
  - `ratio` = `first`'in payı (server `0.1–0.9`'a klempler). `"right"` → first SOL / second SAĞ; `"down"` → first ÜST / second ALT.
  - Export formatı **JSON** (TOML değil). `pane layout` CLI çıktısı ayrı bir `PaneLayoutSnapshot` (rect + split sınırları, varsayılan 120×40 alan).
- Taşıma: named session socket'i `%APPDATA%\herdr\sessions\<ad>\herdr.sock`; Windows'ta named pipe `\\.\pipe\<socket yolu>` (interprocess `GenericNamespaced` verbatim prepend). Protokol = newline-delimited JSON: tek satır `{"id","method","params"}` → tek satır response. Plugin komutlarına server `HERDR_SOCKET_PATH` env'ini enjekte eder (`src/app/api/plugins/runtime.rs:42`).
- İlişkili CLI yüzeyi: `pane split --direction right|down --ratio`, `pane move`, `pane swap`, `pane resize`, `pane layout`, `workspace|tab create/focus/close`.

Kanıt dosyaları: `.local/herdr-api-schema.txt` (bundled schema dump), `.local/loop/lanes/lane08/herdr_api.py`, `verify.js`, `*-verify.json`.

## 2. Plugin — `herdr-plugins/beyn.layouts/`

```
herdr-plugin.toml   manifest: 3 apply action + 1 export action (node layouts.js …)
layouts.js          socket-API client: apply / export / list / ratio
presets/lane-grid.json     5 eş lane (nested right-split 0.2/0.25/0.3333/0.5)
presets/focus-stats.json   focus ~72% + sağda stats(üst %60)/hud(alt %40) şeridi
presets/pair-review.json   2 yan yana ~72% + altta full-width evidence ~28%
README.md                  preset formatı, kullanım, ne-zaman-hangisi
```

Eş lane notu: N eş sütun için ratio'lar kalan alanın oranı — `0.2, 0.25, 1/3, 0.5` (README'de açıklı).

## 3. Uygulama kanıtı (HERDR_SESSION=ln08)

Ortam: `herdr server` arka planda, `workspace create --label lane08` → `w1`, alan 120×40.

### lane-grid (`w1:t2` manuel + `w1:t9` verify A; round-trip clone `w1:tA`)
`pane layout` (t2): 5 pane, her biri `24×40` @ x=0,24,48,72,96 — tam eşit.
`pane list`: `lane-1..lane-5` etiketli p2–p6. Splits: right @0.2/0.25/0.3333/0.5.

### focus+stats (`w1:t5`; round-trip clone `w1:t6`)
- focus `86×40` @0,0 (72%) · stats `34×24` @86,0 · hud `34×16` @86,24.
- Etiketler: focus/stats/hud (pH/pJ/pK).

### pair-review (`w1:t7`; round-trip clone `w1:t8`; plugin-invoke `w1:tB`)
- review-left `60×29` @0,0 · review-right `60×29` @60,0 · evidence `120×11` @0,29.
- `plugin action invoke apply-pair-review` → `plugin-log-1` status=succeeded, tB aynı geometriyle oluştu.

## 4. Round-trip protokolü (preset başına)

`layout.apply(preset)` → `pane.layout` + `layout.export` (A) → `layout.apply(exported.root)` taze tab (B) → `pane.layout` + `layout.export` (B) → normalize ağaç eşitliği (eps 1e-5; pane_id/cwd strip) + rect multiset eşitliği → `tab.close` B.

| preset | A | B | verdict |
|---|---|---|---|
| lane-grid | t9 | tA | PASS — rects + tree aynı |
| focus+stats | t5 | t6 | PASS — rects + tree aynı |
| pair-review | t7 | t8 | PASS — rects + tree aynı |

Tam kanıt JSON'ları: `lane08/lane-grid-verify.json`, `focus-stats-verify.json`, `pair-review-verify.json`.

`layout.set_split_ratio` de doğrulandı: `ratio root 0.5` / `ratio 1 0.3` t5 üzerinde etkili, sonra geri alındı (0.72/0.6).

## 5. Temizlik

- `plugin link`→`action invoke`→`plugin unlink` döngüsü tamamlandı; `plugin list` sadece önceden var olan `beyin.stats`'i gösteriyor — global registry'de iz yok.
- ln08 server: `session stop` + `session delete` (raporun sonunda çalıştırıldı).
- Çit korundu: commit/push yok, src/ dokunulmadı, global config yok. Değişen dosyalar sadece `herdr-plugins/beyn.layouts/*`, `.local/loop/lanes/lane08*` çalışma dosyaları ve `.local/herdr-api-schema.txt`.

## Notlar / gotcha

- `layout.apply` "reflow" değil, **yeni tab** (`tab_id` → replace). Mevcut tabı yerinde yeniden şekillendirmek için `tab_id` ile replace akışı kullanılmalı.
- `pane layout`/`pane list` CLI komutları var ama export/apply için socket gerekli → `layouts.js` bu boşluğu kapatıyor (Node `net` modülü named pipe'a bağlanıyor).
- Pane id'ler alfanümerik (`w1:pA`, `w1:p18`) — sıralı sayısal varsayma.
- Headless oturumda `pane layout` alanı sabit 120×40 — geometri kanıtı deterministik.
