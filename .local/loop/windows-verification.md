# Windows doğrulama kuyruğu — sonuçlar

Oturum: Devin/Echo @ SCUM (kasa) · Binary: `target/release/herdr.exe` v0.9.3 (kaynaktan build)
Ortam: headless named session `gece-loop`, workspace `w1`, pane `w1:p1` (powershell→devin)

## B1 — `agent start --kind devin` + `agent prompt` uyumu
**EVET** (2026-10-08)

- `agent start devin-1 --kind devin --pane w1:p1` → `interactive_ready:true`, status `idle`, argv `["devin"]`.
- `agent prompt devin-1 "<metin>" --wait` → status `working`; `agent wait --until done,idle` → `idle`, `completion_seq` arttı.
- `agent read` → Devin CLI v3000.10.27 TUI'si pane'de render edildi, cevap `LOOP-OK` görünür.
- **Bonus:** Devin footer'da `Pro · 48% remaining (resets in 3d 8h)` ve `Context: 24k / 262k tokens (9%)` —
  kota + context verisi ekranda mevcut → `pane read` ile parse edilebilir (beyin.stats için kaynak).

## B2 — agent-exit/respawn'da scrollback
**EVET** (2026-10-08)

- devin-1'e `agent send-keys ctrl+c` ×2 → devin.exe çıktı, foreground `powershell.exe`'ye döndü.
- `pane read --source visible` çıkış sonrası: eski UI frame'i + `Resume this session with devin.exe -r lyrical-land` + shell prompt — hepsi ekranda.
- `agent start devin-2 --kind devin` aynı pane'de respawn → OK.
- `pane read --source recent --lines 60` respawn sonrası: ilk oturumun TAM çıktısı + ikinci `& devin` başlangıcı aynı buffer'da → scrollback exit/respawn'da sıfırlanmıyor.
- Not: `pane process-info` pozisyonel argüman almıyor, `--pane <ID>` şart.

## B3 — agent prompt text size limiti
**SINIR ≈ 30k char** (2026-10-08)

- `agent prompt <target> <text>` argv-pozisyonel; `--file`/`--stdin` YOK (agent.rs:777).
- 1k/8k/30k → `agent_prompted`, devin işledi, `LEN-OK` cevabı döndü.
- 31k/32.6k → `agent_prompted` döner AMA payload bracketed-paste olarak input kutusunda **submit edilmeden kalıyor** (`[Pasted text #N (NNN chars)]` chip'leri). Devin TUI büyük paste'de trailing Enter'ı yutuyor. Temizlik: `send-keys ctrl+u`.
- 34k/40k → input doluyken `agent_prompt_stalled`; 100k → bash `Argument list too long` (OS argv limiti ~32k+overhead).
- **Lane sonucu**: brief ≤30k → `agent prompt`; üstü → dosyaya yaz + "şu dosyayı oku" prompt'u (loop doc'unun dediği gibi). Paste-staging davranışı `pane read` ile tespit edilebilir (`[Pasted text` chip'i input satırında = unstaged).

## B4 — worktree trust_repository Windows'ta gerekli mi + default dizin
**EVET çalışıyor (trust'suz)** (2026-10-08)

- `worktree create --cwd sandbox --branch lane-test --path sandbox-wt` → `--trust-repository` OLMADAN başarılı: workspace `w4`, root pane `w4:p1`, `is_linked_worktree: true`.
- `--trust-repository` flag'i opt-in; gerekmiyor (her iki çağrı da başarılı — w4/w5).
- **Default dizin**: `--path` verilmezse `C:\Users\pc\.herdr\worktrees\<repo_adi>\<branch>` (repo sibling değil, kullanıcı-seviyesi herdr dizini). Test: `lane-default` → `.herdr\worktrees\sandbox\lane-default`.
- `worktree list` sadece mevcut cwd'nin repo'suna ait worktree'leri gösteriyor gibi (beynhdr cwd'sinde sandbox worktree'leri listelenmedi).

## B5 — terminal session observe Windows'ta NDJSON stream veriyor mu
**EVET** (2026-10-08)

- `terminal session observe term_65d48a656006a1 --cols 80 --rows 24` → canlı NDJSON: her satır `{"bytes":"<base64-VT-output>"}`.
- İlk frame tam ekran repaint (devin pane'in ANSI dump'ı). Stream açık kalıyor (timeout ile kesildi, EXIT=124 normal).
- Kullanım: replay/kayıt (asciinema-benzeri) ve evidence-capture için hazır primitif. `pane read` anlık snapshot; `observe` sürekli akış — ikisi farklı iş.
- Not: `HERDR_SESSION` env'i unutulursa default `herdr-client.sock`'a gider → session-adı şart.

## B5 — terminal session observe (NDJSON) — yukarıda, EVET

## B6 — overlay açılırken focus-restore yarışı var mı
**HAYIR (yarış yok) — kod + unit test kanıtlı** (2026-10-08)

- `src/app/api.rs:499-536` `restore_overlay_after_exit`: overlay açılırken `previous_focus` + `previous_zoomed` snapshot'lanıyor; exit'te tek-thread handler'da restore ediliyor — async yarış penceresi yok.
- Edge-case guard'ları: pane ölmüşse `tab.panes.contains_key(&overlay.previous_focus)` kontrolü; kullanıcı overlay açıkken tab değiştirdiyse `was_overlay_active` ile geri zıplamıyor; zoom `previous_zoomed`'a dönüyor.
- Unit test'ler mevcut: `overlay_exit_preserves_focus_changed_before_exit` (api.rs:2012), `overlay_exit_preserves_same_tab_focus_changed_before_exit` (api.rs:2226), agent_resume.rs'de inactive-tab/zoom-hidden varyantları.
- Tek non-doğrulanan yüzey: overlay'in *açılış* anındaki input routing'i interaktif TUI ister — headless'ta elle doğrulanamaz; ama state machine tarafı temiz.
