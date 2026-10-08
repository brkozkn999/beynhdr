# lane07 — `beyn.keys` keybind paketi (lane hızlı-geçiş odaklı)

Tarih: 2026-10-08 · Lane: ln07 · Binary: `target/release/herdr.exe` v0.9.3
Paket: `..\herdr-plugins\beyn.keys\` (`beynhdr.toml` + `herdr-plugin.toml`)

## 1. Config şema özeti (kaynak: `src/config/`)

- **Dosya**: `%APPDATA%\herdr\config.toml`; override env: `HERDR_CONFIG_PATH` (`src/config/io.rs:195-200`, `src/config.rs:56`). Doğrulama: `herdr config check` (exit 0 + `config: ok`, sorun varsa satır satır diagnostic + exit 1).
- **Bind formatı** (`src/config/keybinds.rs:1124-1171`): `"prefix+X"` = prefix modu tuşu; `"ctrl+alt+n"` = doğrudan chord. Değer tek string veya array (`["prefix+n","ctrl+alt+]"]`). Modifier token'ları: `ctrl/shift/alt/cmd/super/meta/hyper` (+`option`,`command`,`control`). İsimli tuşlar: `space enter esc tab backspace left right up down minus comma period slash backslash quote double_quote semicolon colon percent ampersand backtick plus`, tek karakter, `f1..fN`. `shift+tab` → BackTab.
- **Range/indexed**: `"...1..9"` — sadece `switch_tab`, `switch_workspace`, `focus_agent` alanlarında geçerli (sayı tuşu 1-9'a genişler). `[keys.indexed]` legacy tablosu hâlâ parse ediliyor ama yeni config'lerde action alanları tercih edilmeli (docs/next configuration.mdx).
- **Prefix**: `keys.prefix` — default `ctrl+b` (`DEFAULT_PREFIX`, keybinds.rs:14); array kabul eder. Prefix combo'su direct olarak rezerve edilir; bir bind `prefix+<prefix'in kendisi>` olamaz.
- **Güvenlik**: modifiyesiz yazılabilir tuşlar direct bind olarak reddedilir ("unsafe direct keybinding"); `navigate_*` alanları navigate-mode'a özeldir, `prefix+`/`esc`/`enter`/`tab`/ok/sayı kullanamaz.
- **Çakışma kuralı** (keybinds.rs:1079-1122): user bind'ları default'tan önce işlenir; ilk kayıt kazanır, ikinci `"kept X, disabled Y"` diagnostic'iyle düşer. Bir alanı explicit set etmek o alanın default'unu tamamen değiştirir (diagnostic üretmeden).
- **Custom komutlar**: `[[keys.command]]` — `key`, `command`, `type` (`shell`/`pane`/`popup`/`plugin_action`), `description`, `width`/`height` (popup).
- **Bind edilebilir action seti** (model.rs:336-465 — tam liste): prefix, help, settings, new_workspace, new_worktree, open_worktree*, remove_worktree*, rename_workspace, close_workspace, workspace_picker, goto, navigate_{workspace_up,workspace_down,pane_left,pane_down,pane_up,pane_right}, detach, reload_config, open_notification_target, previous_workspace*, next_workspace*, previous_agent*, next_agent*, focus_agent* (indexed), remote_image_paste, new_tab, rename_tab, previous_tab, next_tab, move_tab_previous*, move_tab_next*, switch_tab (indexed), switch_workspace* (indexed), close_tab, rename_pane, edit_scrollback, clear_pane*, copy_mode, focus_pane_{l,d,u,r}, swap_pane_{l,d,u,r}, cycle_pane_next, cycle_pane_previous, last_pane*, split_vertical, split_horizontal, close_pane, zoom (`fullscreen` alias), resize_mode, resize_pane_{l,d,u,r}*, toggle_sidebar.
  `*` = default'ta **boş**. Ayrı bir "agent view toggle" action'ı yok — karşılığı `toggle_sidebar` (agent paneli sidebar'da) + `focus_agent`/`previous_agent`/`next_agent`.

## 2. Paket bind tablosu + çakışma analizi

| Alan | Değer | Default | Durum |
|---|---|---|---|
| `switch_workspace` | `prefix+1..9` | boş | **BİLİNÇLİ OVERRIDE** — `prefix+1..9` default'ta `switch_tab`'in; lane-first akışta workspace'lere verildi |
| `switch_tab` | `prefix+shift+1..9` | `prefix+1..9` | **BİLİNÇLİ OVERRIDE** — sekmeler Shift düzeyine taşındı; explicit re-set olduğu için diagnostic bile üretmez |
| `previous_workspace` | `prefix+comma` | boş | yeni, boş combo — çakışma yok |
| `next_workspace` | `prefix+period` | boş | yeni — çakışma yok |
| `last_pane` | `prefix+semicolon` | boş | yeni (tmux `;`); `prefix+l` zaten focus-right olduğu için default boş bırakılmıştı |
| `open_worktree` | `prefix+alt+o` | boş | yeni — çakışma yok (`prefix+o` = open_notification_target, Alt'siz) |
| `focus_agent` | `prefix+alt+1..9` | boş | yeni; docs önerisiyle aynı — çakışma yok |
| `previous_agent` | `prefix+alt+p` | boş | yeni; `prefix+p` previous_tab'den farklı combo (Alt) |
| `next_agent` | `prefix+alt+n` | boş | yeni; `prefix+n` next_tab'den farklı combo (Alt) |
| `zoom` | `prefix+z` | `prefix+z` | default ile aynı — açıkça sabitlendi |
| `copy_mode` | `prefix+[` | `prefix+[` | default sabitlendi |
| `clear_pane` | `prefix+shift+c` | boş | yeni; `prefix+c` new_tab'den farklı (Shift) |
| `toggle_sidebar` | `prefix+b` | `prefix+b` | default sabitlendi — "agent view toggle" karşılığı |
| `workspace_picker` | `prefix+w` | `prefix+w` | default sabitlendi — lane picker |
| `goto` | `prefix+g` | `prefix+g` | default sabitlendi — session navigator |
| `move_tab_previous` | `prefix+shift+comma` | boş | yeni — çakışma yok |
| `move_tab_next` | `prefix+shift+period` | boş | yeni — çakışma yok |

Değişmeden kalan önemli default'lar (dosyada referans yorumu olarak): `new_workspace=prefix+shift+n`, `new_tab=prefix+c`, `previous_tab/next_tab=prefix+p/n`, `focus_pane_*=prefix+h/j/k/l`, `swap_pane_*=prefix+shift+hjkl`, `split_vertical=prefix+v`, `split_horizontal=prefix+minus`, `close_pane=prefix+x`, `resize_mode=prefix+r`, `help=prefix+?`, `reload_config=prefix+shift+r`. Prefix `ctrl+b` korunur — paket `keys.prefix`'i override etmez.

**Tek override çifti**: `switch_workspace` ↔ `switch_tab`. Diğer 15 satır ya boş default'u doldurur ya da default'u birebir sabitler.

## 3. Doğrulama çıktısı

```
$ HERDR_CONFIG_PATH='...\herdr-plugins\beyn.keys\beynhdr.toml' herdr config check
config: ok            (EXIT=0, sıfır diagnostic)
```

Negatif kontrol (aynı komut, kasıtlı çakışma+bozuk tuş içeren geçici dosya):

```
config: issues found
invalid keybinding: keys.previous_workspace = "notakey!!"; disabling binding
prefix+z: kept keys.close_pane, disabled keys.zoom       (EXIT=1)
```

→ `config check`'in conflict/invalid diagnostic'leri gerçekten raporladığı kanıtlandı; paketin `config: ok` çıktısı "tüm bind'lar parse edildi + internal/default çakışma yok" demektir.

Ek doğrulama: `beynhdr.toml` + `herdr-plugin.toml` Python `tomllib` ile parse edildi (her ikisi OK). Manifest şeması `src/app/api/plugins/manifest.rs`'e göre zorunlu alanları taşıyor (`id` — noktalı id `beyn.keys` geçerli, `name`, `version`, `min_herdr_version = "0.9.0"` ≤ binary 0.9.3); `actions`/`panes` zorunlu değil (config-only pack).

## Kurulum notu

`beynhdr.toml` içindeki `[keys]` bloğunu `%APPDATA%\herdr\config.toml`'a kopyala → `herdr config check` → oturumda `prefix+Shift+R` (veya `herdr server reload-config`). Global config'e bu lane tarafından yazılmadı.
