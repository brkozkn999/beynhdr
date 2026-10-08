# gece loop RAPOR — 2026-10-08

Oturum: `gece-loop` (named headless session, socket: `%APPDATA%\herdr\sessions\gece-loop\herdr.sock`)
Kapsam: A baseline · B Windows doğrulama kuyruğu · C beyin.stats spike · D wikilink parser spike — **4/4 tamamlandı.**

## Biten maddeler

| Madde | Sonuç | Kanıt |
|---|---|---|
| A baseline `just check` | ✅ YEŞİL (3. deneme; ilk 2 çevresel) | `just-check2.log`, exit=0 |
| B1 devin start+prompt | ✅ EVET | `windows-verification.md` |
| B2 exit/respawn scrollback | ✅ EVET — korunuyor | aynı |
| B3 prompt boyut limiti | ≈30k char; 31k+ paste-staging'de kalıyor | aynı |
| B4 worktree trust + default dizin | ✅ EVET trust'suz; default `~/.herdr/worktrees/<repo>/<branch>` | aynı |
| B5 `terminal session observe` | ✅ EVET — NDJSON `{"bytes":"<b64>"}` canlı stream | aynı |
| B6 overlay focus-restore yarışı | HAYIR — tek-thread restore + guard'lar + unit test'ler | `api.rs:499-536`, testler 2012/2226 |
| C `beyin.stats` spike | ✅ ÇALIŞTI | `c-beyin-stats.md`, `herdr-plugins/beyin.stats/` |
| D `beyin.brain` parser spike | ✅ 214 node / 130 edge JSON | `wikilink.js` + `wikilink-graph.json` |

Commit yok: tüm artefaktlar `.local/` (gitignore) veya `../herdr-plugins/` (repo dışı). `src/` core'a hiç dokunulmadı — merge borcu sıfır.

## Doğrulama kuyruğu sonuç tablosu

Yukarıdaki tablo = kapanış raporu. En değerli bulgular:

- **B3 sınır**: >30k brief `agent prompt`'la gidiyor ama devin'in input'unda unstaged paste olarak kalıyor (Enter yutuluyor). → Lane tasarımı: uzun brief dosyaya, pane'e "dosyayı oku" prompt'u. Bu zaten varsayımdı; artık kanıtlı + davranış tespit edilebilir (`[Pasted text` chip'i input'ta).
- **B5 `observe`**: NDJSON byte-stream Windows'ta çalışıyor → receipt/replay ve "pane time-lapse" (rekabet raporu §7 imza fikri) için hazır primitif.
- **Metadata → view yolu**: `pane report-metadata --source beyin.stats --token k=v` token'ları `pane get` + `api snapshot`'a düşüyor; `agent.view.set` (socket-only, `AgentViewField::Token` filter) ile lane/dashboard filtreleme mümkün. **Kota panosu için fork'a gerek yok** — plugin yüzeyi yeterli, fork'u preview pane'e saklarız.

## C detayları (beyin.stats)

- `collect.js` üç kaynak parse ediyor: devin footer (`NN% remaining (resets in …)` + `Context:`), claude `~/.claude/projects/**/*.jsonl` usage, codex `rollout-*.jsonl` token_count (+rate_limits+credits).
- 9 token yazıldı ve kanıtlandı. Nüans: token'ların `--ttl-ms` desteği var; devin kota banner'i her zaman render edilmiyor → kolektör toleranslı parse + kaynak-provenance etiketi şart (rapor §6'nın dediği gibi).
- `plugin action invoke` context'i *focused* pane'den alıyor → otomasyonla arka-plan pane'ine yazarken doğrudan `pane report-metadata` CLI kullan (subagent bunu kesti).
- `costUSD` bu makinenin claude transcriptlerinde yok; `rate_limits` statusline JSON'u da yok → fallback'ler çalışıyor ama kasada Claude Code hiç kurulu değil (binary yok, veri dizini var).

## Takılanlar

1. WinGet Links shim'leri bash/PS PATH'ine düşmüyor → zig + bun elle PATH/env. Tekrarlanabilir env `results.md` Tur 5'te.
2. `pane read` JSON formatı yok (text|ansi) — parser tarafı regex olacak; `api snapshot` zaten JSON (oradan okumak daha sağlam).
3. `worktree list` cwd'nin repo'suna bağlı gibi — cross-repo liste için ayrı çağrı gerekir.

## Senden beklenen kararlar (Berk)

1. **beynhdr'ın ilk native farkı ne olsun?** C spike kota panosunu plugin'le yaptı; native'i hak eden adaylar: preview pane yüzeyi, lane lifecycle (worktree+branch+pane paketi), veya kota panosunun sidebar-native hali. Rekabet raporu §9 sıralaması: F1 kota panosu → F2 attention → F3 lane+receipt.
2. **`herdr-plugins/` repo mu olsun?** Şu an `Code Projects/herdr-plugins/` git'siz sibling. Beynhdr monorepo'suna `plugins/` olarak mı taşıyalım, ayrı repo mu?
3. **Devin pane'i açık bırakıldı** (w1:p1, idle). Session'ı canlı tutuyorum — sabah incelemek istersen `HERDR_SESSION=gece-loop herdr` ile attach olabilirsin.
4. gece-loop-prompt.md'deki "PATH taze shell'de çözülür" varsayımı yanlış çıktı — vault'taki dosyaya düzeltme notu ekleyeyim mi?

## `just check` son durumu

✅ exit=0 — 3375 nextest + 150 unittest + bun test + fmt/clippy, HEAD `master` temiz (tek untracked: `graft/` index dizini).

---

# DALGA 2 — 15 şerit: araştır + geliştir + test (Berk uyurken)

## Build şeritleri (8) — hepsi çalışır artefakt, hepsi testli

| Şerit | Ürün | Doğrulama |
|---|---|---|
| `beyin.stats` v2 | loop modu + `*_src` provenance + staleness + TTL/seq chunking + atomic state | 14/14 test, ln01'de 22-token canlı kanıt |
| `beyn.lane` | brief→worktree→workspace→pane→agent→evidence→receipt runner | dry-run + pane-run + 1 canlı devin E2E |
| `beyn.receipt` | `terminal session observe` NDJSON → ANSI-strip transcript + sha256 zincir | 5 marker bulundu, CHAIN INTACT, tamper→BROKEN |
| `beyn.notify` | status diff → toast (NotifyIcon) + needs-you.json + focus-susturma | E2E ln04, toast API exit=0 |
| `beyn.hud` | `api snapshot` → pane-içi canlı tablo (PANE\|AGENT\|STATUS\|SÜRE\|TOKENS\|LANE) | ln05'te pane'de canlı görüldü, 8/8 test |
| `beyn.parse` | devin/claude/codex parser lib + gerçek fixture'lar | 15/15 test |
| `beyn.keys` | lane-first keybind pack (`prefix+1..9`→workspace) | `config check` sıfır diagnostik |
| `beyn.layouts` | 3 preset (lane-grid/focus+stats/pair-review) | 3/3 export→apply round-trip PASS |

## Araştırma şeritleri (7) — raporlar `.local/loop/research/`

| Şerit | Çekirdek bulgu |
|---|---|
| lane09 devin-surface | **Kota diskten okunuyor:** `user_status.*.bin` protobuf → daily%/weekly%/overage/reset-ts/acu_limit. Screen-parse plan B. |
| lane10 claude-model | Kota% diske yok (statusline stdin persist değil); token'lar `message.id` dedupe şart; `.claude.json` son-oturum `lastCost`; bu makinede provider=codex-proxy, model `gpt-5.6-*` → kendi fiyat tablosu lazım |
| lane11 codex-model | `token_count` event = tam kota paketi (usage + rate_limits{used_percent,resets_at} + credits + plan_type); dosya saati lokal/içerik UTC tuzağı |
| lane12 plugin-gaps | Tam yetenek haritası. Gap: preview-pane=**FORK**, lane+ajan-bus=**SINIRDA**, kota/HUD/toast=**PLUGIN**. Sidebar satırları client-local config (plugin değer verir, satır ekleyemez) |
| lane13 upstream-drift | 3 commit geride, ff-merge mümkün; 2 commit bizim alanlara dokunuyor (hyperlink/mouse → preview; windows-cwd → lane) |
| lane14 event-map | Token değişimi `pane.updated`/`workspace.metadata_updated` ile **akıyor** (kota panosu subscribe-only olabilir); `pane.output_changed` ölü schema; metadata restart'ta persist değil → reconnect'te yeniden report |
| lane15 attention-map | 18 kanal envanteri; WinRT toast `OpenConsole.exe` sidecar ister (dev binary'de yok); öneri: `report-agent blocked` + state-label + tek `notification show --sound request` |

## Dalga-2 kritik bulgular (tasarımı değiştirenler)

1. **`agent wait --until idle` yanılgısı** — prompt kabulünden sonra pre-prompt idle'a match oluyor; doğru `done|blocked` (lane02).
2. **Devin kotası diskte** — protobuf `user_status.*.bin` decode edildi; en sağlam devin kaynağı bu (lane09).
3. **`report-agent` reap** — bilinmeyen label + shell prompt'a dönüş → authority ~1s'de düşer; simülasyon `custom:` source/known label ile (lane04).
4. **Pane içinden API** — pane'ler `HERDR_SOCKET_PATH`+`HERDR_BIN_PATH` inherit ediyor → HUD/mini-app'ler pane içinde çalışabilir, `HERDR_SESSION` gerekmez (lane05).
5. **Metadata event'leniyor** — token panosu poll'suz, `events.subscribe`'la çalışır (lane14).
6. **Sidebar provisioning eksik** — plugin token basar ama sidebar satırını kullanıcının config'i tanımlar → "tek kurulumda pano belirir" ya config-hack ya fork (lane12).
7. **Upstream drift ff-able** — hemen merge etsek sıfır çatışma; iki commit bizim alanlara dokunuyor (lane13).

## Güncellenmiş öncelik önerisi (kanıta dayalı)

- **F1 kota panosu = plugin'le tam ürün**: stats-v2 + hud + keys + layouts zaten %80'i. Sidebar satırı için kullanıcı config snippet'i ship et (veya küçük bir `beyn.setup` config-writer).
- **F2 lane runner**: primitifler kanıtlı; `agent wait` bug'ı documented; atomiklik istenirse çekirdek.
- **F3 receipt**: observe + hash-chain çalışıyor; sonraki adım VT-replay (lane03 notu).
- **Fork'un gerçek ilk işi**: preview pane (plugin sınırı kesin) + ops. sidebar provisioning + lane kimliği.

## Artefakt envanteri

- `herdr-plugins/`: beyin.stats, beyn.lane, beyn.receipt, beyn.notify, beyn.hud, beyn.parse, beyn.keys, beyn.layouts (git'siz sibling — repo kararı Berk'te)
- `.local/loop/lanes/laneNN-*.md` (8 rapor), `.local/loop/research/laneNN-*.md` (7 rapor)
- `.local/state/` (beyn-stats.json, needs-you.json), `.local/receipts/`, `.local/lanes/` kanıtları
- Fork çalışma ağacı temiz; gece-loop session hâlâ canlı (w1:p1 devin idle).
