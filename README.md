# Kawaii Radio — kompyuter ilovasi (KR-01 DESK)

Windows (keyin macOS/Linux) uchun Kawaii Radio. Telefondagi ilova bilan bir xil akkaunt (Kawaii ID), chat,
pleylistlar va radio — lekin kompyuter uchun qayta chizilgan, premium dizayn.

## Dizayn konseptlari (`design/`)

| Fayl | G'oya |
|---|---|
| `a-studio-deck` | Grafit "studiya pulti": chapda rack, markazda efir (vinil + LCD'da qo'shiq matni), o'ngda jonli chat, pastda pult (VU, ovoz murvati) |
| `b-chalk-hifi` | Telefondagi "chalk" (oq) uslubning kompyuter versiyasi: Braun / Teenage Engineering, bosiladigan tugmalar, tuner shkala |
| `c-cinema` | Kino rejimi (F11): butun ekran, muqova rangidagi fon, sinxron matn va tarjima, to'lqin |

HTML'ni brauzerda ochib ko'rish mumkin, PNG — tayyor skrinshot.

## Texnologiya (taklif)

- **Tauri 2** — Rust yadro, oyna WebView2 (Windows'da tayyor), `.exe` ~10 MB, RAM kam.
- **UI — Leptos (Rust → WASM)**: kawaii-bot saytidagi bilan bir xil; animatsiyalar CSS/WebGL (GPU), 60–144 FPS.
- **Ovoz — Rust** (`symphonia` + `cpal`): dekodlash, ekvalayzer, spektr/VU uchun haqiqiy FFT, gapless.
- Tizim: global media tugmalari, Windows "Now Playing" paneli (SMTC), tray, Discord Rich Presence.
- Backend — mavjud radio.kawaii.uz (Cloudflare Worker): playlist, chat (WebSocket), Kawaii ID, yangilanishlar.

## 3D-dizayn (Blender) — asosiy yo'nalish

Ekran — haqiqiy qurilma: `design/blend/` dagi skriptlar Blender (Cycles) da render qiladi, 1 birlik = 100 px,
kamera ortografik, shuning uchun har bir element UI koordinatasiga aniq tushadi.

| Skript | Nima |
|---|---|
| `lib.py` | Sahna, ikki rang (chalk / graphite), materiallar, yumaloq qutilar, matn, piksel ikonkalar |
| `main.py` | Asosiy ekran 1920×1080: `--theme light/dark`, `--print 0..1` (chek), `--anim-print dir` |
| `jukebox.py` | Muqovalar oynasi: perspektiv 3D "jukebox", `--anim dir` — trek almashish animatsiyasi |
| `encode.py` | Kadrlar → MP4 |

Blender: `.tools/blender-4.5.9-windows-x64/blender.exe` (git'ga kirmaydi).
Shriftlar: Michroma (yozuvlar), IBM Plex Sans (matn), DotGothic16 / VT323 (ekranlar), Galmuri11 (menyu).

## Ilova (`app/`) — v0.1

Tauri 2 (Rust) oyna + `app/ui/` dagi qatlam. Ekran — Blender render'lari (`ui/assets/<mavzu>-up|down.webp`,
`main.py --app [--down]` bilan olinadi): "up" — fon, "down" dan bosilgan tugma / yongan LED bo'lagi kesib olinadi.
Ustida jonli: trivision muqova, qo'shiq matni (karaoke), LCD (vaqt, to'lqin), baraban, ovoz murvati (LED halqa),
VU-ustun (haqiqiy signal), TAVSIF cheki (o'rama), chat (o'qish), qidiruv / kutubxona / profil / sozlamalar modullari.

Jonli efir: `GET /radio?station=jpop|ost|riot` jadvali bo'yicha, hamma bilan bir xil soniyada.

### Yig'ish (Windows, admin huquqisiz)

```bash
export RUSTUP_HOME=/d/kawaii_radio_exe/.tools/rustup CARGO_HOME=/d/kawaii_radio_exe/.tools/cargo
export PATH="/d/kawaii_radio_exe/.tools/cargo/bin:/d/kawaii_radio_exe/.tools/mingw64/bin:$PATH"
cd app && cargo build --release      # → app/target/release/kawaii-radio.exe (~10 MB) + WebView2Loader.dll (yonma-yon turishi shart)
```
Rust — `x86_64-pc-windows-gnu`, linker va `windres` — WinLibs MinGW (`.tools/mingw64`).
Fon render'larini yangilash: `blender -b -P design/blend/main.py -- --theme dark --app [--down] --out app/ui/assets/dark-up.png` → WebP.

### Keyingi qadamlar
- Kawaii ID bilan kirish (kompyuter uchun: brauzer → bulut → ilova so'rovi), chatda yozish, LEVEL/RANK.
- O'Z TANLOVIM (Kawaii Pass), yuklab olish (SAQLASH).
- O'rnatuvchi (NSIS) va avto-yangilanish.
