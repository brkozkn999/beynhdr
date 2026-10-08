# Lane 02 — beyn.lane: brief → izole lane → kanıt receipt runner'ı

## Sonuç: YEŞİL — dry-run E2E + komutlu tam akış + canlı devin prompt'u (1 adet) doğrulandı

```
HERDR_SESSION=ln02 (izole server), repo = .local\loop\sandbox
node lane.js run --brief <md> --repo <sandbox> --name <lane> [--agent devin] [--dry-run] [--run <cmd>...]
```

## Dosyalar

```
herdr-plugins/beyn.lane/
  lane.js             — runner: worktree create → (ops.) agent start/prompt/wait → kanıt → receipt.json
  herdr-plugin.toml   — plugin manifesti (beyin.stats ile aynı şema)

Kanıtlar (sandbox/.local/lanes/<ad>-<ts>/):
  drya-20261008-002023/   — --dry-run: sadece worktree+workspace+receipt
  fullb-20261008-002032/  — --run ile 2 komut: dosya oluşturma + git status
  devc-20261008-002057/   — --agent devin: tek satırlık iş, canlı prompt
```

## Akış (doğrulanan)

1. `herdr worktree create --cwd <repo> --branch lane/<name>` → JSON `result`'tan
   `workspace.workspace_id`, `root_pane.pane_id`, `worktree.path`, `worktree.branch` alınır (tahmin yok).
2. `--run <cmd>` tekrarlanabilir → her biri `pane run <pane> <cmd>` (pane PowerShell, cwd=worktree).
3. `--agent <kind>` → `agent start <name> --kind <kind> --pane <p>` → `agent prompt <p> <brief>`
   (≤30k inline; >30k ise "dosyayı oku" prompt'u, `prompt_mode:"file"`) → `agent wait --until done --until blocked --timeout`
   (arka planda; eşzamanlı `pane get` poll'ü `status_seq`'i doldurur).
4. Kanıt: `pane read --source recent` + `git -C <wt> status --short` + `git -C <wt> diff --stat` + `git worktree list`
   → `.local/lanes/<name>-<ts>/{pane-recent.txt,git-status.txt,git-diff-stat.txt,git-worktree-list.txt,receipt.json}`.

## Doğrulama sonuçları

- **(a) --dry-run** `drya`: receipt üretildi; `git worktree list`'te `lane-drya [lane/drya]` gerçekten vardı; workspace `w3`, pane `w3:p1`; `status_seq=[unknown]`; agent alanı null. ✓
- **(b) agentsız tam akış** `fullb`: iki `--run` komutu pane'de çalıştı; `git-status.txt` kanıtında `?? lane-artifact.txt`; `pane-recent.txt`'te komut satırları görünüyor; worktree'de dosya fiziksel olarak var. ✓
- **(c) canlı devin** `devc` (maks 1 prompt — tek prompt kullanıldı): `agent start` → devin v3000.10.27 açıldı (`interactive_ready:true`); `agent prompt` → `agent_prompted`; devin brief'i işleyip `devin-lane.txt` yazmak için izin diyaloğuna düştü (`blocked`); diyalog `pane send-keys 1` ile onaylandı → `agent wait` `done` döndü. `devin-lane.txt` = "devin was here" ✓. Yan artefakt: devin'in kendi `graft/.cache/` session klasörü (iş çıktısı değil).

## Bulunan gerçek bug + fix

- **`agent wait` idle-race**: prompt kabulünden hemen sonra agent hâlâ `idle` → `--until idle` listeye girerse wait anında döner
  (devc'de ~4.3sn'de `idle` döndü, agent sonra working→blocked'a gitti). Fix: until listesi `done|blocked`
  (lane.js:277-286, yorum satırında açıklama). Devin'in `idle↔working` geçişinde `idle`'ı hiç hedeflememek gerekiyor.
- `worktree remove --workspace <w> --force` worktree+workspace'i kapatır ama **branch'i silmez** → temizlik `git branch -D` ile ayrıca.
- `agent prompt <pane> <text>` target olarak pane id (`w5:p1`) kabul ediyor; agent name de olur.

## Receipt şeması

```json
{lane, branch, worktree, workspace, pane,
 agent:{kind,target,prompt_mode,prompt_chars,start,prompt_result,wait_result,wait_exit},
 commands_run:[{cmd,exit}], status_seq:[{t,status}], evidence_files:[...],
 brief:{path,bytes}, out_dir, dry_run, created_at, finished_at}
```

`devc` receipt'ine `post_receipt_completion` eklendi: ilk wait'in idle'a takılması (fix öncesi davranış) +
diyalog onayı sonrası `done` + yenilenen kanıt dosyaları — şeffaf işaretli.

## Temizlik

`worktree remove --force` (w3/w4/w5) + `git branch -D lane/{drya,fullb,devc}` → `git worktree list` sadece master;
sandbox'ta sadece untracked `.local/` (receipt'ler) kaldı. `session stop ln02` + `session delete ln02` yapıldı; server çıktı.
Geçici `patch-receipt.js` silindi. Brief input'ları `.local/loop/sandbox-brief{,-devin}.md` duruyor (receipt'ler referanslıyor).

## Çit

Commit/push yok; `src/` yok; global config/override yok. Yazılan yerler: `herdr-plugins/beyn.lane/`,
`sandbox/.local/lanes/`, `.local/loop/` scratch input'ları, bu rapor. Diğer session'lara (gece-loop, ln03-05) dokunulmadı.
