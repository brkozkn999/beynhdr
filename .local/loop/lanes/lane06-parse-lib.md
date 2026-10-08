# Lane 06 — beyn.parse: paylaşılan kota/kullanım parse kütüphanesi

## Sonuç: YEŞİL — `node --test` 15/15 pass, 0 fail (≈70 assertion)

```
cd "C:\Users\pc\Documents\Code Projects\herdr-plugins\beyn.parse" && node --test
# tests 15  # pass 15  # fail 0   (node v22.20.0)
```

## Dosyalar

```
herdr-plugins/beyn.parse/
  parse.js          — 4 export, pure parsing (I/O yok, herdr bağımlılığı yok)
  parse.test.js     — node:test + node:assert/strict, 15 test
  fixtures/
    claude-usage.jsonl        — 2 GERÇEK satır (bkz. kaynaklar)
    claude-no-usage.jsonl     — 1 gerçek file-history-snapshot satırı (negatif)
    codex-token-count.jsonl   — 1 GERÇEK token_count event satırı
    codex-other.jsonl         — 1 gerçek task_started event satırı (negatif)
    devin-footer-full.txt     — Pro banner + Context + Model + Update
    devin-footer-context-only.txt — banner scroll'dan düşmüş varyant
    devin-footer-paste-chip.txt   — [Pasted text #1 (32619 chars)] varyantı
    devin-usage-screen.txt    — /usage ekranı (Plan/ACU satırlı)
```

## API sözleşmesi

Her export `null` (tanınmadı) veya `{value, unit, source, agent, confidence, ...alanlar}` döndürür.
`value`/`unit` = kaynağın birincil metriği; `confidence` `'high'` (güçlü anchor) / `'low'` (zayıf sinyal).

| Fonksiyon | source | Birincil value | Ek alanlar |
|---|---|---|---|
| `parseDevinFooter(text)` | `footer` | kota `% remaining`; yoksa context `%` | `quota{plan,percentRemaining,resetsIn,resetsInMinutes}`, `context{usedText,limitText,usedTokens,limitTokens,percentUsed}`, `model`, `updateAvailable`, `pasteChips[{index,chars}]` |
| `parseDevinUsageScreen(text)` | `usage_screen` | `% remaining` veya ACU used | `plan`, `quota`, `resets`, `acu{used,limit}`, `context` |
| `parseClaudeJsonlLine(line)` | `jsonl` (agent:`claude`) | totalTokens = in+cache_create+cache_read+out | `model`, `messageId`, `timestamp`, `sessionId`, `usage{…}` |
| `parseCodexRolloutLine(line)` | `jsonl` (agent:`codex`) | `info.total_token_usage.total_tokens` | `total`, `last`, `modelContextWindow`, `rateLimits{primary,secondary,credits{balance},planType}` |

Null koşulları: geçersiz/bozuk JSON, `usage`/`token_count` yok, tüm token sayaçları 0, footer'da hiçbir pattern eşleşmedi.

## Kullanılan gerçek fixture'lar

- **Claude**: `C:\Users\pc\.claude\projects\C--Users-pc\0787ee81-…\….jsonl` satır 12 (`gpt-5.6-sol`, in=34731 out=126) + `97ce809c-…\….jsonl` son cache'li satır (cache_read=34304, in=1424, out=52). Negatif: aynı dosyadan `file-history-snapshot` satırı.
- **Codex**: `C:\Users\pc\.codex\sessions\2026\08\09\rollout-2026-08-09T15-06-13-….jsonl` satır 336 (`total_tokens`=3563021, `primary.used_percent`=16, `window_minutes`=10080, `credits.balance`="2062.517235", `plan_type`="plus", `model_context_window`=258400). Negatif: aynı rollout'tan `task_started` event.
- **Devin footer**: gerçek metinler `loop/windows-verification.md` + `results.md`'den (`Pro · 48% remaining (resets in 3d 8h)`, `Context: 24k / 262k tokens (9%)`, `Context: 24k/262k (9%)` varyantı, `SWE-2 Max`, `Update v3000.11.3 available!`, `[Pasted text #1 (32619 chars)]`).

## Notlar / tasarım kararları

- `k`/`M` soneki ×1000/×1e6 (Devin "262k" = 262144'ün ondalık gösterimi).
- `resets in 3d 8h` → `resetsInMinutes` 4800 (d/h/m/s parse edilir).
- Claude `<synthetic>` model veya model yok → confidence `low`; `message.usage` VEYA top-level `usage` kabul.
- Codex `payload.type==="token_count"` şart (event_msg wrap'i opsiyonel); info yoksa `low` + credits'e düşer.
- `/usage` gerçek capture'ı bu makinede yok — parser banner formu + `Plan:`/`ACU` satır varyantlarını tolere eder; tek zayıf sinyalde `low`.
- Entegrasyon: `beyin.stats/collect.js`'teki inline regex'ler bu lib'e taşınabilir (quota/context/claude-usage/codex token_count eşdeğer karşılıkları mevcut).
- Çit korundu: commit/push yok, vault'a yazılmadı; sadece `herdr-plugins/beyn.parse/` + bu rapor.
