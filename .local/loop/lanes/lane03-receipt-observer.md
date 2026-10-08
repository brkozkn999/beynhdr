# lane03 — `beyn.receipt` observer spike

Tarih: 2026-10-08 · Oturum: `ln03` (named session, doğrulama sonrası stop+delete edildi)
Kapsam: pane canlı çıktısı → hash-zincirli kanıt kaydı. **Tamamlandı: 3/3 + negatif test.**

## Ne yazıldı

| Dosya | İş |
|---|---|
| `herdr-plugins/beyn.receipt/observe.js` | `<pane_id>` → `pane get` ile `terminal_id` çöz → `terminal session observe` spawn → NDJSON parse → base64 decode → ANSI strip → `transcript.txt` append. Her N observe-satırı (default 10) veya flush'ta receipt chunk'ı |
| `herdr-plugins/beyn.receipt/verify.js` | Receipt jsonl'ı okur; `chunk_b64` decode + `bytes` uzunluk + `sha256` recompute + `prev_sha256` link + `seq` sürekliliği kontrol eder. Exit 0 = INTACT, 1 = BROKEN |
| `herdr-plugins/beyn.receipt/transcript.txt` | ANSI'siz canlı metin akışı (3.245 byte) |
| `herdr-plugins/beyn.receipt/.local/receipts/w1_p1-2026-10-08T00-20-14-154Z.jsonl` | 5 satırlık zincirli receipt (7.391 byte) |

Receipt satır şeması: `{seq, ts, reason, lines, bytes, sha256, prev_sha256, chunk_b64}` — görevdeki 5 alanın üstünde `reason`/`lines`/`chunk_b64` var; `chunk_b64` receipt'i kendi kendine yeten (self-contained) kanıt yapar, verify chunk'ı yeniden hash'leyebilir. Genesis `prev_sha256 = "0"*64`.

## Doğrulama kanıtları

`pane run w1:p1` ile 5 ayırt edici komut (PowerShell pane'i):

```text
$ grep -o "MARKER-[A-Za-z0-9-]*" transcript.txt | sort | uniq -c
  1 MARKER-42MARKER-42PSC
  1 MARKER-99-ALPHAMARKER-99-ALPHAPSC
  1 MARKER-1791418821MARKER-1791418821PSC
  1 MARKER-BRAVO-7MARKER-BRAVO-7PSC
  1 MARKER-CHARLIE-finalMARKER-CHARLIE-finalPSC
```

Transcript excerpt: `…echo MARKER-42` → `MARKER-42` → `PS C:\Users\pc>` (komut eko'su + çıktı + prompt, VT delta fragmanları olarak).

```text
$ node verify.js .local/receipts/w1_p1-2026-10-08T00-20-14-154Z.jsonl
line 1: OK  seq=1 bytes=3690 sha256=5c5b2da10f2043ac…   prev=0000000000000000… (genesis)
line 2: OK  seq=2 bytes=205  sha256=f5f2f25a06073dc7…   prev=5c5b2da10f2043ac…
line 3: OK  seq=3 bytes=203  sha256=bb66e3daf564b347…   prev=f5f2f25a06073dc7…
line 4: OK  seq=4 bytes=346  sha256=ab1e2f0187c0e782…   prev=bb66e3daf564b347…
line 5: OK  seq=5 bytes=128  sha256=c937949950ce9f84…   prev=ab1e2f0187c0e782…
---
lines=5 total_bytes=4572
CHAIN INTACT   (exit=0)
```

Flush nedenleri gözlendi: `chunk-lines` (5-satır eşiği) + `flush-ms` (8sn timer) — ikisi de tetiklendi.

**Negatif test (tamper):** receipt'in 3. satırında `chunk_b64`'ün ilk byte'ı flip edildi:

```text
line 3: FAIL sha256 mismatch: recomputed=5d46936a… recorded=bb66e3da…
CHAIN BROKEN   (exit=1)
```

## Bulgular / nüanslar

1. **`observe` çıktısı tarif edilenden zengin**: satırlar `{"bytes":"<b64>","encoding":"ansi","full":bool,"height":N,"seq":N,"type":"terminal.frame","width":N}` — `terminal.frame` objesi. `full:true` ilk frame'de tam ekran snapshot'ı taşıyor (80×24 ~3.7KB), sonrakiler incremental delta.
2. **`pane get` alanı `terminal_id`** (`result.pane.terminal_id`), `attached_terminal_id` değil — observe.js ikisini de dener.
3. **Transcript fragmanlı**: delta'lar "cursor'a şu metni yaz" parçaları → substring marker araması çalışır, tam ekran/prose rekonstrüksiyonu için VT parser gerekir (replay/time-lapse spike'ının doğal devamı).
4. **Windows kill = hard kill**: `taskkill`/`kill_shell` node'a SIGTERM iletmez → kapanışta `final` flush'u garanti değil; `--flush-ms` >0 bırakmak gözetimli koşularda kaybı ≤timer aralığına indirir. (Bu koşuda tüm chunk'lar zaten flush'lanmıştı.)
5. `pane run` komutu pane'in shell'ine (powershell) yazıyor; `echo MARKER-x` hem eko hem çıktı olarak iki kez düşüyor — marker sayımında `uniq -c` çift görür, bu beklenen.

## Sınır uyumu

- Commit/push yok; `src/` yok; global config/override yok.
- Dokunulan alanlar: `herdr-plugins/beyn.receipt/` (observe.js, verify.js, transcript.txt, `.local/receipts/`) + bu rapor. `beynhdr/.local/` altına sadece bu md yazıldı.
- `ln03` session `server stop` + `session delete` ile temizlendi; `session list`'te diğer lane session'ları (ln02/ln04/ln05/ln08/ln15, gece-loop) duruyor, ln03 yok.

## Kullanım

```bash
HERDR_SESSION=<s> node observe.js <pane_id> [--chunk-lines N] [--flush-ms N] [--cols N] [--rows N]
node verify.js <receipt.jsonl>
```
