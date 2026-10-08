# lane01 — beyin.stats collector v2

Spike (`c-beyin-stats.md`) üzerine genişletme; mevcut tek-shot akış ve flag'ler korundu.

## Yapılanlar

- **`collect.js` v2 (`version = "0.2.0"`)** — parser'lar saf fonksiyonlara ayrıldı
  (`parseDevinFooter`, `parseClaudeStatusline`, `parseClaudeTranscript`,
  `parseCodexRollout`) ve `require.main === module` guard + `module.exports` ile
  test edilebilir hale getirildi.
- **`--loop --interval <sec>`** (default 30s): her tick collect →
  `pane report-metadata`. Ekstra: `--ticks <n>` (0=sonsuz; doğrulama için),
  `--quiet` (per-tick log'u kapat).
- **Provenance**: her metrik token yanına `<key>_src` sibling token'ı
  (`footer|jsonl|screen|estimate`). Devin için önce `agent read` (src=`footer`),
  olmazsa `pane read` fallback'i (src=`screen`). Claude/Codex dosya tabanlı
  (src=`jsonl`); `stats_at`/`quota_age_s` türetilmiş (src=`estimate`).
- **Staleness**: `quota_age_s` = son başarılı quota okumasının yaşı (saniye).
  `last_quota_ok_ms` state dosyasında persist; bir tick'te quota kaynağı taze
  veri üretirse sıfırlanır, üretemezse yaş büyümeye devam eder. Kaynağı o tick
  okunamayan metrikler state file'daki per-source cache'ten son bilinen
  değerleriyle yeniden raporlanır (src korunur; `sources.<name>.cached=true`).
- **`--ttl-ms` + monoton `--seq`**: schema'ya uygun (key `[A-Za-z0-9_-]` ≤32,
  value ≤80, ≤16 token/istek). Token+src çiftleri 16'lık chunk'lara bölünüp her
  chunk'a `seqBase+i` gönderilir; `seqBase = max(Date.now(), last_seq+1)` ile
  cross-run monoton (state'de `last_seq` persist). Loop'ta default TTL =
  3×interval (tek-shot'ta flag verilmezse TTL yok — spike davranışı korundu).
- **State file**: her tick tam snapshot → atomik (tmp+rename) yazım, dizin yoksa
  `mkdirSync recursive`. Path: `--state-file` > `$BEYIN_STATE_FILE` >
  `cwd/.local/state/beyn-stats.json`. İçerik: version, updated_at, ts_ms, tick,
  seq/last_seq, target_pane, ttl_ms, quota_age_s, tokens, provenance,
  quota_keys, per-source {ok,src,cached,last_ok_ms,tokens,prov,quota},
  reported/report_batches.
- **`herdr-plugin.toml`**: version 0.2.0; yeni `collect-loop` action'ı
  (`node collect.js --loop`).
- **Testler**: `collect.test.js` — 14 birim test (`node --test`), parser'lar +
  fmtK + token-schema + 16'lık chunking + state round-trip + sourceOfKey.
  Fixture'lar: `beynhdr/.local/loop/fixtures/` (devin-footer.txt,
  devin-screen-nofooter.txt, claude-transcript.jsonl, claude-statusline.json,
  codex-rollout.jsonl). Fixture dir çözümü: `$BEYIN_FIXTURE_DIR` >
  `../../beynhdr/.local/loop/fixtures` > plugin-içi `fixtures/`.

## Kanıtlar

### `node --test` yeşil
```
# tests 14 / # pass 14 / # fail 0   (duration_ms ~79)
```

### Tek-shot (ln01, sahte footer `pane run` ile basıldı)
Pane `w1:p1` (powershell): `Write-Output 'Pro 63% remaining (resets in 2d 4h)'; Write-Output 'Context: 41k / 128k tokens (32%)'`.
Not: `agent read` agent'siz pane'de `agent_not_found` veriyor → fallback
`pane read` devreye girdi, devin src = `screen`.

`node collect.js --pane w1:p1 --devin-pane w1:p1 --print` çıktısı (20 entry, 2 batch):
```
devin_quota=63% left          devin_quota_src=screen
devin_resets=2d 4h            devin_resets_src=screen
devin_ctx=32% of 128k         devin_ctx_src=screen
claude_in=1.8k / claude_out=2.5k / claude_cache=73.2k   *_src=jsonl
codex_week_used=16% / codex_credits=2063 / codex_total=3.6M  *_src=jsonl
stats_at=03:24 stats_at_src=estimate
quota_age_s=0 quota_age_s_src=estimate
report-metadata -> w1:p1: ok (2 batch(es), seq 1791419089022..1791419089023)
```

`pane get w1:p1` → `title:"beyin.stats"`, 22 token; örnek:
```json
{"devin_quota":"63% left","devin_quota_src":"screen","quota_age_s":"0",
 "quota_age_s_src":"estimate","codex_week_used_src":"jsonl", ...}
```

### Loop modu (3 tick, interval 4s) — state JSON her tick güncellendi
```
[00:25:07.116Z] tick=1 tokens=11 batches=2 reported=true seq=...106584 quota_age_s=0
[00:25:11.723Z] tick=2 tokens=11 batches=2 reported=true seq=...111136 quota_age_s=0
[00:25:16.314Z] tick=3 tokens=11 batches=2 reported=true seq=...115739 quota_age_s=0
```
State dosyası iki okuma arasında farklı: tick2 `updated_at=00:25:11.723Z
ts_ms=1791419111723` → tick3 `updated_at=00:25:16.314Z ts_ms=1791419116314`
(~4.6s fark), `ttl_ms=12000` (3×interval).

### Staleness/cache yolu
`--devin-pane w9:p9` (var olmayan pane) tek-shot: devin okuması başarısız →
`sources.devin = {ok:false, src:"screen", cached:true, last_ok_ms:...115760}` ve
`devin_*` token'ları cache'ten `src=screen` ile yeniden raporlandı
(`pane get`'te `devin_quota=63% left` + `devin_quota_src=screen` duruyor).
`quota_age_s` codex quota'sı hâlâ taze olduğu için 0'a resetlendi (design:
son başarılı *quota* okumasının yaşı, kaynaklar arası min).

## Takılanlar / notlar

- `agent read` sadece agent'lı pane'lerde çalışıyor (`agent_not_found`);
  powershell pane'de `pane read` fallback'i zaten kodda vardı ve src=`screen`
  olarak işaretlendi — provenance bu sayede E2E'de de kanıtlandı.
- 16 token/istek limiti `*_src` eklenince tek-shot'ta bile aşılıyor → chunk'lı
  report (2 batch, artan seq) zorunlu oldu; `pane get` tümünü doğru gösteriyor.
- `report-metadata` başarıda stdout'a yazmıyor (spike notu) — exit-code
  kontrolü aynen korundu.
- Tek-shot modda TTL default yok (spike davranışı); loop'ta 3×interval.
  `--ttl-ms` her iki modda da override edilebilir.
- ln01 session: `session stop ln01` + `session delete ln01` yapıldı;
  `session list`'te ln01 yok (diğer lane'lerin session'larına dokunulmadı).

## Dosyalar

- `herdr-plugins/beyin.stats/collect.js` (v2, genişletildi)
- `herdr-plugins/beyin.stats/collect.test.js` (yeni — 14 test)
- `herdr-plugins/beyin.stats/herdr-plugin.toml` (0.2.0 + collect-loop action)
- `beynhdr/.local/loop/fixtures/{devin-footer.txt,devin-screen-nofooter.txt,claude-transcript.jsonl,claude-statusline.json,codex-rollout.jsonl}` (yeni)
- `beynhdr/.local/state/beyn-stats.json` (loop snapshot artifact)
- `beynhdr/.local/loop/lanes/lane01-stats-v2.md` (bu rapor)
