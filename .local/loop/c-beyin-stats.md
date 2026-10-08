# C — beyin.stats plugin spike

## Yapılanlar
- `herdr-plugins/beyin.stats/` olusturuldu: `herdr-plugin.toml` (id=beyin.stats, `[[actions]] collect` contexts=[workspace,pane], `[[panes]] hud` overlay) + `collect.js` (Node, tek-shot).
- Manifest formati `tests/fixtures/plugin-smoke` + `docs/plugins.mdx` + `src/app/api/plugins/manifest.rs` + `api_helpers.rs`'ten dogrulandi (token key: [A-Za-z0-9_-] <=32, value <=80, <=16 token/istek, source: alnum+`:._-`, seq monoton).
- `collect.js` 3 kaynak topluyor: (a) Devin footer `agent read` regex `NN% remaining (resets in …)` + `Context: Nk/Mk (N%)`; (b) Claude en yeni `~/.claude/projects/**/*.jsonl` usage; (c) Codex en yeni `rollout-*.jsonl` `token_count` (rate_limits + credits).
- `workspace create --label stats` → `w2`, pane `w2:p1`. `agent start --kind devin --pane w2:p1` ile kendi pane'imde canli Devin baslattim (w1:p1'e dokunmadim; quota banner'i orada scroll'dan dusmustu).
- `plugin link` OK, `plugin list`'te `beyin.stats enabled [local]`, `plugin action list` → `beyin.stats.collect`. (`plugin pane` altinda list yok; `plugin list`'te `panes:[hud]` gorunuyor.)
- `node collect.js --pane w2:p1` → `pane report-metadata --source beyin.stats` 9 token yazdi.

## Kanit (pane get w2:p1)
```json
"title":"beyin.stats","tokens":{"claude_cache":"73.2k","claude_in":"1.8k","claude_out":"2.5k","codex_credits":"2063","codex_total":"3.6M","codex_week_used":"16%","devin_quota":"48% left","devin_resets":"3d 8h","stats_at":"02:57"}
```
`api snapshot` panes[] icinde de ayni token'lar duruyor.

## Notlar / takilanlar
- `report-metadata` basarida stdout'a hicbir sey basmiyor (exit 0) — script once "FAILED" sandi, null-check ile duzeltildi.
- Devin quota banner'i persistent degil; `--lines 12` dar kaldi, 60'a cikarildi. `plugin action invoke` context'i focused pane'den aliyor (w1:p1) → baska seridin pane'ine yazmamak icin invoke edilmedi; tek-shot `node collect.js` ile dogrulandi.
- `costUSD` bu makinedeki Claude transcriptlerinde yok; `~/.claude` altinda statusline `rate_limits` JSON'i da yok → JSONL usage fallback'i kullanildi.

## Dosyalar
- `C:\Users\pc\Documents\Code Projects\herdr-plugins\beyin.stats\herdr-plugin.toml`
- `C:\Users\pc\Documents\Code Projects\herdr-plugins\beyin.stats\collect.js`
