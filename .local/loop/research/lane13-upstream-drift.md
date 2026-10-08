# Lane 13 — Upstream Drift Envanteri (beynhdr ← herdrdev/herdr)

Tarih: 2026-10-08 · Yöntem: yalnızca `git fetch upstream` + okuma (merge/rebase/commit/push YOK).

## 0. Fork noktası ve durum

| Ref | Commit | Tarih |
|---|---|---|
| `master` (fork, local) | `376bafc8` | 2026-10-07 23:18 +0300 |
| `upstream/master` (fetch öncesi) | `376bafc8` | — |
| `upstream/master` (fetch sonrası) | `4dc23bb1` | 2026-10-08 01:30 +0200 |

- `origin/master` remote-ref'i local'de yok (sadece fetch etmedik origin'den); fork point = local `master` = `376bafc8`.
- `git rev-list --count upstream/master..master` = **0** → fork'ta henüz hiç yerel commit yok. Durum = saf fast-forward; bugün merge edilirse çatışma riski fiilen sıfır.
- Tek untracked: `graft/` (index dizini) + `.local/` — merge'i etkilemez.

## 1. Yeni commit listesi (3 commit, +653/−97, 20 dosya)

```
4dc23bb1 fix: respect mouse capture when routing hyperlink clicks (#5064)   2026-10-08
4f296c29 fix: preserve windows shell working directories (#5062)            2026-10-08
848075ff fix: retain workspace navigation during machine switches (#5061)   2026-10-07
```

### Commit bazında alan dağılımı

| Commit | Alan(lar) | Dosyalar | Planlanan dokunuşla kesişim |
|---|---|---|---|
| `4dc23bb1` hyperlink/mouse-capture | app/api (plugin), client/shell, docs, testler | `src/app/api/plugins/mod.rs` (+67), `src/client/shell/{actions,link_hover,mouse}.rs`, `src/client/state.rs` (+18), `src/client/shell/tests/*` (+~250 test), `docs/**/plugins.mdx`+`quick-start.mdx` (en/ja/zh-cn) | ⚠️ **preview/overlay** (input routing + `state.rs` frame sunumu) ve **plugin link path** |
| `4f296c29` Windows shell cwd | platform, pane | `src/platform/windows.rs` (+217/−31, ağır rework), `src/pane.rs` (4 satır), `src/platform/mod.rs` (+3) | ⚠️ **lane** (pane cwd = lane paketi parçası) + `process_cwd → pane_process_cwd` rename |
| `848075ff` workspace nav/machine switch | client shell_runtime, endpoint testleri | `src/client/shell_runtime.rs` (+21), `src/client/endpoint/activation_tests.rs`, `src/client/shell/tests/endpoints.rs` | △ zayıf — **agent view/workspace switching** komşuluğu |

### Alan özeti (istenen kategoriler)

- **detect/manifests**: YOK — hiçbir değişiklik.
- **api/schema**: YOK — `src/api/schema/` bu 3 commit'te dokunulmamış (v0.9.3'ten beri de değişmedi, bk. §4).
- **app/**: 1 commit — `src/app/api/plugins/mod.rs` (plugin link-handler hata yolu: `false` → `encode_error(id,"plugin_link_failed",err)` + test).
- **server/**: YOK — `src/server/` hiç dokunulmamış.
- **plugin**: `src/app/api/plugins/mod.rs` + `plugins.mdx` dokümanları.
- **docs**: `plugins.mdx` + `quick-start.mdx` ×3 dil (en/ja/zh-cn) — plugin doc snippet'leri.
- **ci**: YOK — `.github/` hiç dokunulmamış.
- **kategori dışı ama önemli**: `src/client/` tarafı (TUI input/link/mouse/state/shell_runtime) = drift'in ana kütlesi; `src/platform/windows.rs` Windows cwd rework'ü.

### Planlanan dokunuş alanlarıyla kesişim matrisi

| Bizim alan | Kod yüzeyi | Drift'te dokunan dosya | Çatışma riski |
|---|---|---|---|
| pane metadata (token/report) | `src/app/api/panes.rs`, `src/cli/pane.rs`, `src/api/schema/panes.rs`, `src/app/agent_view.rs` | — (yok) | düşük |
| agent view (`AgentViewField`, `agent.view.set`) | `src/app/agent_view.rs`, `src/app/api/agent_view.rs`, `src/agent_view_eval.rs` | — (yalnız test dosyası `shell/tests/endpoints.rs` komşu) | düşük |
| preview/overlay | `src/client/shell/overlays.rs`, `src/client/shell/{input,mouse,actions}.rs`, `src/client/state.rs` | `actions.rs`, `link_hover.rs`, `mouse.rs`, `state.rs` (+testler) | **orta** — aynı dosyalar; hyperlink/mouse-capture mantığı overlay input routing'le aynı katman |
| lane (worktree+branch+pane) | `src/app/api/worktrees*`, `src/pane.rs`, `src/platform/windows.rs`, `src/client/shell/worktrees.rs` | `src/pane.rs`, `src/platform/{mod,windows}.rs` | **orta** — `process_cwd→pane_process_cwd` rename + windows.rs'de geniş rework; lane cwd'ye dokunursa çatışır |

## 2. Agent-detection TOML'ları (fork point'ten beri)

**YOK.** `master..upstream/master` aralığında:
- `src/detect/manifests/*.toml`: 0 dosya değişti.
- `distribution/agent-detection/*`: 0 dosya değişti (index.toml dahil).
- `devin.toml` dahil hiçbir TOML'a dokunulmadı. `devin.toml`'un son upstream değişikliği `07261e09` "feat: add devin cli detection and restore support (#622)" (2026-06-15) — zaten fork'ta.
- `src/detect/` engine kodu da tamamen dokunulmamış.

## 3. API schema / protocol — son release'ten (v0.9.3, 2026-09-29) beri

Son stable: **v0.9.3** (`7b116c05`, tag 2026-09-29). `v0.9.3..upstream/master` = **54 commit**.

| Soru | Cevap |
|---|---|
| Protocol bump? | **HAYIR — hâlâ 22.** `src/protocol/wire.rs:20` `PROTOCOL_VERSION = 22`, hem v0.9.3'te hem upstream/master'da aynı. |
| `src/api/schema/` değişti mi? | **HAYIR** — v0.9.3'ten beri sıfır diff (14 schema dosyasının tamamı). |
| Endpoint fixture'ları? | `tests/fixtures/endpoint-{hello,method-shapes,snapshot,welcome}-v1.json` — v0.9.3'ten beri dokunulmamış (gen-1 sözleşme stabil). |
| `src/protocol/wire.rs` | −32 satır net: `ClientInputEvent::from_crossterm` taşındı + test güncelleme; wire shape değişikliği yok. |
| `src/api/server.rs` | +79/−12 — implementasyon (dispatch/handler), schema değil. |
| `src/client/endpoint/activation*` | Yeni dosyalar (+~247, çoğu test) — endpoint activation path; internal client tarafı. |
| `src/app/api/` | +1107/−163 — handler tarafı büyümüş (agents +183, workspaces +169, layouts +149, plugins/runtime +238, worktrees +110/+124 rework) ama schema tipleri sabit. |

**Özet**: protocol 22→22, schema değişmedi, gen-1 endpoint kontratı korunuyor. Client fork'u wire-uyumlu kalıyor; risk sadece `src/app/api/` handler implementasyonunda (bizim pane-metadata/agent-view işi `src/app/api/{panes,agent_view}.rs`'a girerse kümülatif drift burada birikiyor — v0.9.3'ten beri 1100+ satır).

## 4. Merge borcu tahmini

- **Bugün**: 3 commit, ~650 satır, fork'ta 0 yerel commit → temiz fast-forward, borç ~sıfır.
- **Planlanan işler başlarsa**: 3 commit'in ~2'si potansiyel çatışma alanında (`4dc23bb1` → preview/overlay client-shell dosyaları; `4f296c29` → lane pane/platform dosyaları). `pane_process_cwd` rename'i küçük ama isim-bazlı çatışma üretir; `windows.rs` rework'ü geniş.
- **Hız**: v0.9.3'ten beri 54 commit / ~10 gün ≈ **5-6 commit/gün**. Bu gecikme (1 gün) bile 3 commit üretti.

### Önerilen senkron ritmi

1. **Şimdi**: `git merge --ff-only upstream/master` (veya rebase eşdeğeri) ile 3 commit'i al — bedava, bekletmeye değmez. Özellikle `4f296c29` (Windows cwd) bizim lane işinin zemini; geç kalınca windows.rs drift'i büyür.
2. **Ritim**: günlük `fetch upstream` + **haftalık (veya lane-PR'ı açmadan önce) ff-merge**. 5-6 commit/gün hızında 2 haftalık gecikme ~70 commit ≈ windows.rs/pane.rs'de gerçek çatışma olasılığı.
3. **Alan-bazlı tetik**: `src/pane.rs`, `src/platform/windows.rs`, `src/client/shell/`, `src/app/api/{panes,agent_view,worktrees}.rs` dosyalarına upstream commit'i düştüğü an o lane'in sync'ini öne çek (bu raporun kesişim tablosu = tetik listesi).
4. Agent-detection TOML ve schema tarafı sakin — o alanlar için ritim gevşek tutulabilir; ama AGENTS.md'nin `distribution/agent-detection` kuralı gereği manifest değişikliği yaparsak catalog-alignment kontrolü şart.

## 5. Yapılan işlemler (denetim izi)

- `git fetch upstream` → `376bafc8..4dc23bb1 master`, yeni branch `issue/4472-windows-cwd`.
- Okuma komutları: `rev-list`, `log`, `diff --stat/--name-only`, `show`, `merge-base --is-ancestor`, `ls-remote --tags`, `grep` (PROTOCOL_VERSION, AgentViewField, report_metadata).
- Yazma: yalnızca bu rapor dosyası. Commit/push/merge/rebase/worktree YOK. Çalışma ağacı ve index değişmedi (`graft/` untracked önceden vardı).
