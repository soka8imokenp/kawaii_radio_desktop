/* KR-01 DESK — the live layer on top of the rendered device.
 * Coordinates are the 1920 x 1080 design ones (design/blend/main.py); #stage is scaled to the window. */
'use strict';
const API = 'https://radio.kawaii.uz';
const VERSION = '0.6.3';
const $ = (s) => document.querySelector(s);
const stage = $('#stage');
const audio = $('#audio');
const tauri = window.__TAURI__;
const invoke = (c, a) => (tauri && tauri.core ? tauri.core.invoke(c, a) : Promise.resolve());

/* ───────────── settings kept between runs */
const store = {
  get(k, d) { try { const v = localStorage.getItem('kr.' + k); return v == null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem('kr.' + k, JSON.stringify(v)); } catch {} },
};
const S = {
  theme: store.get('theme', 'dark'),
  station: store.get('station', 'jpop'),
  volume: store.get('volume', 0.72),
  lang: 'rom',
  liked: new Set(store.get('liked', [])),
  playing: false,
  mode: 'live', // 'live' or 'pick'
};

/* ───────────── scale the device into the window
 * Whatever shape the window gets (dragged, maximized, snapped to a half / quarter / third of the screen) the
 * device fills it: close to 16:9 it is stretched the last few percent (invisible), otherwise it is fitted whole
 * and centred on the chassis colour. */
function fit() {
  const W = innerWidth, H = innerHeight;
  let sx = W / 1920, sy = H / 1080;
  const r = (W / H) / (16 / 9);
  if (r < 0.92 || r > 1.09) { sx = sy = Math.min(sx, sy); }
  stage.style.transform = `translate(${(W - 1920 * sx) / 2}px, ${(H - 1080 * sy) / 2}px) scale(${sx}, ${sy})`;
  stage.dataset.scale = Math.max(sx, sy);
  stage.style.setProperty('--px', (1 / (sy * (devicePixelRatio || 1))).toFixed(4) + 'px');
}
addEventListener('resize', fit);
fit();

function applyTheme() {
  stage.classList.toggle('light', S.theme === 'light');
  stage.classList.toggle('dark', S.theme !== 'light');
  document.body.classList.toggle('light', S.theme === 'light');
}
applyTheme();

/* ───────────── controls: hit areas with the "down" render behind them */
const ctls = $('#controls');
function ctl(id, x0, y0, x1, y1, opt = {}) {
  const d = document.createElement('div');
  d.className = 'ctl' + (opt.cls ? ' ' + opt.cls : '');
  d.id = id;
  Object.assign(d.style, { left: x0 + 'px', top: y0 + 'px', width: x1 - x0 + 'px', height: y1 - y0 + 'px' });
  const lit = document.createElement('i');
  lit.className = 'lit';
  lit.style.backgroundPosition = `${-x0}px ${-y0}px`;
  d.append(lit);
  if (opt.title) d.title = opt.title;
  if (opt.click) d.addEventListener('click', (e) => { click(); opt.click(e); });
  ctls.append(d);
  return d;
}
const led = (id, cx, cy, r) => ctl(id, cx - r, cy - r, cx + r, cy + r, { cls: 'led noclick' });
const on = (el, v) => el && el.classList.toggle('on', !!v);

// a soft mechanical click
let actx;
let click = function () {
  try {
    actx = actx || new AudioContext();
    const t = actx.currentTime, o = actx.createOscillator(), g = actx.createGain();
    o.type = 'square'; o.frequency.setValueAtTime(1800, t); o.frequency.exponentialRampToValueAtTime(400, t + 0.03);
    g.gain.setValueAtTime(0.05, t); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.04);
    o.connect(g).connect(actx.destination); o.start(t); o.stop(t + 0.05);
  } catch {}
};

// nav
// (no CHAT key: the chat is always on screen, bottom right) — screen ids stay: 0 home, 1 search, 3 library, 4 profile
const NAV = [['ASOSIY', 0], ['QIDIRUV', 1], ['KUTUBXONA', 3], ['PROFIL', 4]];
const navKeys = NAV.map(([n, id], k) => Object.assign(ctl('nav' + k, 20, 22 + k * 104, 108, 118 + k * 104, {
  title: n, click: () => openScreen(id),
}), { _id: id }));
on(navKeys[0], true);
const gearKey = ctl('gear', 20, 978, 108, 1058, { title: 'Sozlamalar', click: () => (screen === 5 ? openScreen(0) : openSettings()) });
const meter = Array.from({ length: 18 }, (_, j) => led('m' + j, 64, 548 + j * 22, 10));

// band selector (RADIO / O'Z TANLOVIM)
const slider = ctl('slider', 836, 34, 1368, 102, { title: "Radio / O'z tanlovim", click: () => toggleMode() });

// stations
const STATIONS = [['jpop', 'J-POP FM'], ['ost', 'OST FM'], ['riot', 'RIOT FM']];
const stKeys = STATIONS.map(([key, freq], i) => ctl('st' + i, 152 + i * 216, 112, 360 + i * 216, 178, {
  title: freq, click: () => tuneStation(key),
}));


// deck
const playKey = ctl('play', 130, 836, 242, 992, { title: 'Ijro / pauza', click: () => togglePlay() });   // a toggle: lever up = playing
const heartKey = ctl('heart', 769, 861, 871, 963, { cls: 'round', title: 'Sevimli', click: () => toggleLike() });
const tavsifKey = ctl('tavsif', 128, 982, 498, 1058, { title: 'Tavsif', click: () => printReceipt() });
const saveKey = ctl('saqlash', 508, 982, 878, 1058, { title: 'Saqlash', click: () => toast('SAQLASH · TEZ ORADA') });
ctl('prev', 1030, 856, 1102, 960, { title: 'Oldingi', click: () => {
  if (S.mode === 'pick') {
    if (audio.currentTime > 3) { audio.currentTime = 0; toast('BOSHIDAN BOSHLANDI'); }
    else playPrevTrack();
  } else {
    toast("JONLI EFIR · ORQAGA YO'Q");
  }
}});
ctl('next', 1030, 956, 1102, 1062, { title: 'Keyingi', click: () => {
  if (S.mode === 'pick') playNextTrack();
  else toast("JONLI EFIR · OLDINGA YO'Q");
}});
const ring = Array.from({ length: 21 }, (_, i) => {
  const a = (225 - i * 13.5) * Math.PI / 180;
  return led('r' + i, 1250 + Math.cos(a) * 86, 958 - Math.sin(a) * 86, 9);
});
const printerLed = led('pled', 1866, 50, 9);
// the monitor's input row: round ID key, an LCD strip, round smile key
ctl('cid', 1440, 1007, 1480, 1047, { cls: 'round', title: 'Kawaii ID', click: () => (kid.acc ? openScreen(4) : kidLogin()) });
ctl('csm', 1834, 1007, 1874, 1047, { cls: 'round', title: 'Stikerlar', click: () => (kid.acc ? stickerTray() : kidLogin()) });

/* ───────────── window keys and dragging by the chassis
 * Any surface that is not a control moves the window (Windows' own move loop: Snap to edges / corners / the
 * Snap Layouts bar works); a double click there maximizes / restores, like a title bar. */
const NO_DRAG = '.ctl, input, textarea, a, #panel, #paperWin, #chCompose, #stTray, #chMsgs, #chPin, #rz, #lcd';
document.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  const t = e.target;
  if (!(t instanceof Element) || t.closest(NO_DRAG)) return;
  e.preventDefault();
  if (e.detail === 2) invoke('window_action', { action: 'maximize' });
  else invoke('start_drag');
});
addEventListener('keydown', (e) => {
  if (e.code === 'Space' && document.activeElement.tagName !== 'INPUT') { e.preventDefault(); togglePlay(); }
  if (e.key === 'Escape') openScreen(0);
  if (e.key === 'F11') { e.preventDefault(); invoke('window_action', { action: 'fullscreen' }); }
});

let toastT;
function toast(text) {
  const t = $('#toast');
  t.textContent = text;
  t.classList.add('on');
  clearTimeout(toastT);
  toastT = setTimeout(() => t.classList.remove('on'), 1800);
}

/* ───────────── Kawaii ID state & pass check */
const kid = { token: store.get('kidToken', null), acc: store.get('kidAcc', null), you: null };
function hasKp() {
  const a = kid.acc;
  if (!a) return false;
  if (a.kpUntil && a.kpUntil > Date.now()) return true;
  if (a.adm) return true;
  if (a.roles && a.roles.length) return true;
  if (kid.you && (kid.you.admin || kid.you.owner)) return true;
  return false;
}

function toggleMode(want) {
  const target = want || (S.mode === 'pick' ? 'live' : 'pick');
  if (target === 'pick') {
    if (!hasKp()) {
      toast("O'Z TANLOVIM · KAWAII PASS BILAN");
      return;
    }
    S.mode = 'pick';
    on(slider, true);
    stationUi();
    toast("O'Z TANLOVIM · QO'SHIQ TANLANG");
    if (screen !== 1 && screen !== 3) openScreen(1);
    syncListening();
  } else {
    S.mode = 'live';
    on(slider, false);
    stationUi();
    tune(true);
    toast(`JONLI EFIR · ${stationName()}`);
    syncListening();
  }
}

/* ───────────── data: tracks and the live timetable */
const tracks = new Map();
let schedule = { slots: [], skew: 0, station: null };
let cur = null; // { track, slot }

async function loadTracks() {
  const r = await fetch(API + '/playlist.json', { cache: 'no-cache' });
  const j = await r.json();
  for (const t of j.tracks) tracks.set(t.id, t);
}
async function loadSchedule(st) {
  const r = await fetch(`${API}/radio?station=${st}`, { cache: 'no-store' });
  const j = await r.json();
  schedule = { slots: j.slots || [], skew: j.serverTime - Date.now(), station: st };
}
const liveNow = () => Date.now() + schedule.skew;
function slotNow() {
  const now = liveNow();
  return schedule.slots.find((s) => s.start <= now && now < s.start + s.duration && tracks.has(s.id));
}
const url = (p) => (p && p.startsWith('http') ? p : API + p);

async function tuneStation(st) {
  S.mode = 'live';
  on(slider, false);
  if (st !== S.station || !schedule.slots.length) {
    S.station = st; store.set('station', st);
    stationUi();
    await loadSchedule(st);
  }
  stationUi();
  tune(true);
}
function stationUi() {
  STATIONS.forEach(([k], i) => on(stKeys[i], S.mode !== 'pick' && S.station === k));
}

function currentList() {
  if (screen === 1 && searchResults.length) return searchResults;
  if (screen === 3 && S.liked.size) return [...S.liked].map((id) => tracks.get(id)).filter(Boolean);
  return [...tracks.values()];
}
function playNextTrack() {
  const list = currentList();
  if (!list.length) return;
  const idx = cur ? list.findIndex((t) => t.id === cur.track.id) : -1;
  const next = list[(idx + 1 + list.length) % list.length];
  playPick(next);
}
function playPrevTrack() {
  const list = currentList();
  if (!list.length) return;
  const idx = cur ? list.findIndex((t) => t.id === cur.track.id) : -1;
  const prev = list[(idx - 1 + list.length) % list.length];
  playPick(prev);
}

function playPick(t) {
  if (!t) return;
  S.mode = 'pick';
  on(slider, true);
  stationUi();
  cur = { track: t, slot: null };
  if (audio.dataset.id !== t.id) {
    audio.dataset.id = t.id;
    audio.src = url(t.audioSrc);
  }
  audio.currentTime = 0;
  S.playing = true;
  on(playKey, true);
  audio.play().catch(() => {});
  ensureAnalyser();
  onTrack(t);
  toast(`${t.title.toUpperCase()} · O'Z TANLOVIM`);
  document.querySelectorAll('#pList .row').forEach((el) => el.classList.toggle('now', el.dataset.id === t.id));
}

function tune(force) {
  if (S.mode === 'pick') return;
  const slot = slotNow();
  if (!slot) { loadSchedule(S.station).then(() => slotNow() && tune(true)); return; }
  const t = tracks.get(slot.id);
  const changed = !cur || cur.track.id !== t.id;
  if (changed || force) {
    cur = { track: t, slot };
    if (audio.dataset.id !== t.id) {
      audio.dataset.id = t.id;
      audio.src = url(t.audioSrc);
    }
    seekLive();
    if (S.playing) audio.play().catch(() => {});
    if (changed) onTrack(t);
  }
}
function seekLive() {
  if (!cur || S.mode === 'pick') return;
  const at = (liveNow() - cur.slot.start) / 1000;
  const go = () => { try { audio.currentTime = Math.max(0, at); } catch {} };
  if (audio.readyState >= 1) go(); else audio.addEventListener('loadedmetadata', go, { once: true });
}

function togglePlay() {
  S.playing = !S.playing;
  on(playKey, S.playing);
  if (S.playing) {
    if (S.mode === 'pick') {
      audio.play().catch(() => {});
      ensureAnalyser();
    } else {
      tune(true);
      audio.play().catch(() => {});
      ensureAnalyser();
    }
  } else {
    audio.pause();
  }
  syncListening();
}
audio.addEventListener('ended', () => setTimeout(() => {
  if (S.mode === 'pick') playNextTrack();
  else tune(true);
}, 200));

// keep in step with the broadcast; roll over to the next song; refresh the timetable when it runs short
setInterval(() => {
  if (S.mode === 'pick') return;
  if (!schedule.slots.length) return;
  const last = schedule.slots[schedule.slots.length - 1];
  if (last.start - liveNow() < 20 * 60 * 1000) loadSchedule(S.station);
  const s = slotNow();
  if (s && (!cur || s.id !== cur.track.id)) tune(true);
  else if (S.playing && cur && !audio.paused) {
    const want = (liveNow() - cur.slot.start) / 1000;
    if (Math.abs(audio.currentTime - want) > 3) seekLive();
  }
}, 1000);

/* ───────────── a new song: cover, LCD, lyrics, receipt, like */
let onTrack = function onTrack(t) {
  trivision(url(t.coverArt));
  $('#lyTitle').textContent = t.title;
  $('#lyArtist').textContent = t.artist;
  parseLyrics(t);
  renderLyrics(true);
  wave = waveOf(t.id);
  on(heartKey, S.liked.has(t.id));
  buildReceipt(t, false);
  if ('mediaSession' in navigator) {
    navigator.mediaSession.metadata = new MediaMetadata({ title: t.title, artist: t.artist, album: 'Kawaii Radio · ' + (S.mode === 'pick' ? "O'z tanlovim" : stationName()), artwork: [{ src: url(t.coverArt), sizes: '512x512' }] });
  }
  syncListening();
  reportPlayTimer(t.id);
}
const stationName = () => (STATIONS.find(([k]) => k === S.station) || STATIONS[0])[1];
if ('mediaSession' in navigator) {
  navigator.mediaSession.setActionHandler('play', () => { if (!S.playing) togglePlay(); });
  navigator.mediaSession.setActionHandler('pause', () => { if (S.playing) togglePlay(); });
}

function toggleLike() {
  if (!cur) return;
  const id = cur.track.id;
  if (S.liked.has(id)) S.liked.delete(id); else S.liked.add(id);
  store.set('liked', [...S.liked]);
  on(heartKey, S.liked.has(id));
  toast(S.liked.has(id) ? 'SEVIMLILARGA QO\'SHILDI' : 'SEVIMLILARDAN OLINDI');
}

/* ───────────── trivision: 15 three-sided slats (convex printed faces, a dark gap between), a wave of 120° turns */
const tv = $('#tv');
const NS = 15, TW = 436, SW = TW / NS, AP = SW / (2 * Math.tan(Math.PI / 3));
let tvStep = 0;
const slats = [];
for (let i = 0; i < NS; i++) {
  const s = document.createElement('div');
  s.className = 'slat';
  s.style.transitionDelay = (i * 38) + 'ms';
  const faces = [0, 1, 2].map((k) => {
    const f = document.createElement('div');
    f.className = 'face';
    f.style.transform = `rotateY(${k * 120}deg) translateZ(${AP}px)`;
    f.style.backgroundPosition = `${-i * SW}px 0`;
    s.append(f);
    return f;
  });
  tv.append(s);
  slats.push({ s, faces });
}
function trivision(src) {
  const img = new Image();
  img.onload = () => {
    const next = tvStep + 1;
    const k = ((next % 3) + 3) % 3;
    slats.forEach(({ s, faces }) => { faces[k].style.backgroundImage = `url("${src}")`; });
    // first cover: no wave
    if (!tv.dataset.ready) {
      tv.dataset.ready = '1';
      slats.forEach(({ s }) => { s.style.transition = 'none'; s.style.transform = `rotateY(${-next * 120}deg)`; void s.offsetWidth; s.style.transition = ''; });
    } else {
      slats.forEach(({ s }) => { s.style.transform = `rotateY(${-next * 120}deg)`; });
      tick(6);
    }
    tvStep = next;
  };
  img.src = src;
}

/* ───────────── lyrics */
let lines = { uz: [], rom: [], jp: [] };
function parseLrc(text) {
  const out = [];
  for (const raw of (text || '').split('\n')) {
    const m = raw.match(/^\s*\[(\d+):(\d+(?:\.\d+)?)\]\s*(.*)$/);
    if (m) out.push({ t: +m[1] * 60 + +m[2], s: m[3] });
  }
  return out.filter((l) => l.s.trim());
}
function parseLyrics(t) {
  const L = t.lyrics || {};
  lines = { uz: parseLrc(L.uzbek), rom: parseLrc(L.romaji), jp: parseLrc(L.kanji) };
}
function activeLines() {
  const l = lines[S.lang];
  if (l && l.length) return l;
  return lines.uz.length ? lines.uz : lines.rom.length ? lines.rom : lines.jp;
}
let lastIdx = -2;
function renderLyrics(force) {
  const box = $('#lyLines');
  const L = activeLines();
  const t = audio.currentTime || 0;
  if (!L.length) {
    if (force || lastIdx !== -1) box.innerHTML = cur ? '<div class="none">♪ MATNSIZ · SPEKTR</div>' : '';
    lastIdx = -1;
    $('#lyRail').innerHTML = '';
    return;
  }
  let i = L.findIndex((l, k) => t >= l.t && (k === L.length - 1 || t < L[k + 1].t));
  if (i < 0) i = t < L[0].t ? -1 : L.length - 1;
  if (force || i !== lastIdx) {
    lastIdx = i;
    // every line keeps its own element, so moving up, growing and dimming are CSS transitions, not a redraw
    const want = [];
    const at = (k, top, size, color, cls = '') => {
      if (k < 0 || k >= L.length) return;
      want.push({ key: 'l' + k, cls: 'ln ' + cls, top, style: `font-size:${size}px;color:${color}`, html: cls === 'now' ? `<span class="k">${esc(L[k].s)}</span>` : esc(L[k].s) });
    };
    at(i - 2, 8, 19, '#5d6446'); at(i - 1, 46, 21, '#8c9367');
    // the sung line always fits in two lines: long lines get a smaller size (Roboto Mono is 0.6em wide, the box ~640px)
    const len = L[i] ? [...L[i].s].length : 0;
    const size = Math.min(42, Math.max(25, Math.floor(1980 / Math.max(1, len))));
    const wrap = len * size * 0.6 > 652 ? 2 : 1;
    const h = wrap * size * 1.22;
    if (i >= 0) at(i, 104, size, '#f2e49c', 'now'); else want.push({ key: 'wait', cls: 'ln', top: 104, style: 'font-size:34px;color:#6d7450', html: '♪ ♪ ♪' });
    // translation under the sung line
    let hasTr = false;
    if (i >= 0 && S.lang !== 'uz' && lines.uz.length) {
      const ti = lines.uz.reduce((b, l, k) => (l.t <= L[i].t + 0.5 ? k : b), -1);
      if (ti >= 0) { hasTr = true; want.push({ key: 't' + ti, cls: 'tr', top: Math.round(104 + h + 10), style: '', html: esc(lines.uz[ti].s) }); }
    }
    const nb = Math.round(104 + (i >= 0 ? h : 40) + (hasTr ? 52 : 40));
    at(i + 1, nb, 22, '#cdc98b'); at(i + 2, nb + 42, 20, '#9a9f72'); at(i + 3, nb + 78, 17, '#5c6247');
    if (force) box.innerHTML = '';
    const keep = new Set(want.map((w) => w.key));
    for (const el of [...box.children]) {
      if (el.dataset.k && keep.has(el.dataset.k) && !el.classList.contains('gone')) continue;
      if (el.classList.contains('gone')) continue;
      // lines leaving drift up and fade out
      el.classList.add('gone');
      el.style.top = (parseFloat(el.style.top) || 0) - 34 + 'px';
      el.style.opacity = '0';
      setTimeout(() => el.remove(), 700);
    }
    for (const w of want) {
      let el = [...box.children].find((e) => e.dataset.k === w.key && !e.classList.contains('gone'));
      if (!el) {
        el = document.createElement('div');
        el.dataset.k = w.key;
        el.className = w.cls;
        el.style.cssText = w.style;
        el.style.top = w.top + 36 + 'px';   // new lines rise in from below
        el.style.opacity = '0';
        el.innerHTML = w.html;
        box.append(el);
        void el.offsetWidth;
      }
      if (el.className !== w.cls) el.className = w.cls;
      if (el._html !== w.html) { el.innerHTML = w.html; el._html = w.html; }
      const top = w.top + 'px';
      el.style.cssText = w.style;
      el.style.top = top;
      el.style.opacity = '';
    }
    // rail
    const n = Math.min(14, L.length), pos = Math.max(0, Math.round((i / Math.max(1, L.length - 1)) * (n - 1)));
    $('#lyRail').innerHTML = Array.from({ length: n }, (_, k) => `<i class="${k === pos ? 'on' : ''}" style="top:${k * 22}px"></i>`).join('');
  }
  // karaoke fill of the sung line
  if (i >= 0) {
    const a = L[i].t, b = i + 1 < L.length ? L[i + 1].t : a + 5;
    const p = Math.max(0, Math.min(1, (t - a) / Math.max(0.5, b - a)));
    const k = box.querySelector('.now:not(.gone) .k');
    if (k) k.style.setProperty('--p', (p * 100).toFixed(1) + '%');
  }
}
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

/* ───────────── the big LCD: title, time, waveform, footer */
const lcd = $('#lcd').getContext('2d');
const LW = 792, WV = LW - 54;   // the LCD is narrower since the VU meters took the left of the deck; the waveform runs 27..LW-27
// O'Z TANLOVIM: the waveform is the timeline — click or drag on it to seek, hovering shows the time under the cursor.
// (Live radio can't be seeked: everyone hears the same second.)
const trackDur = () => (cur && cur.track && cur.track.duration) || (isFinite(audio.duration) ? audio.duration : 0);
const canSeek = () => S.mode === 'pick' && trackDur() > 0;
let lcdHover = null, lcdScrub = false;
function lcdFrac(e) {
  const r = $('#lcd').getBoundingClientRect();
  const x = (e.clientX - r.left) / r.width * LW;          // in canvas units
  return Math.max(0, Math.min(1, (x - 27) / WV));
}
function lcdSeek(e) { audio.currentTime = lcdFrac(e) * trackDur(); }
$('#lcd').addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  if (!canSeek()) { toast("JONLI EFIR · O'TKAZIB BO'LMAYDI"); return; }
  lcdScrub = true; lcdSeek(e); click();
});
addEventListener('mousemove', (e) => {
  if (lcdScrub) { lcdSeek(e); lcdHover = lcdFrac(e); return; }
  if (e.target === $('#lcd') && canSeek()) lcdHover = lcdFrac(e); else lcdHover = null;
  $('#lcd').classList.toggle('seek', canSeek());
});
addEventListener('mouseup', () => { lcdScrub = false; });
$('#lcd').addEventListener('mouseleave', () => { if (!lcdScrub) lcdHover = null; });
let wave = [];
function waveOf(id) {
  let h = 2166136261;
  for (const c of id) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  const rnd = () => ((h = Math.imul(h ^ (h >>> 15), 2246822507) ^ Math.imul(h ^ (h >>> 13), 3266489909)) >>> 0) / 4294967296;
  return Array.from({ length: 180 }, (_, i) => Math.min(54, 8 + Math.abs(Math.sin(i * 0.43 + rnd()) * 30 + Math.sin(i * 0.13) * 16 + rnd() * 10)));
}
const fmt = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
let drawLcd = function drawLcd() {
  const c = lcd, W = LW, H = 152;
  c.clearRect(0, 0, W * 4, H * 4);
  const ink = '#232b1b', ghost = 'rgba(35,43,27,.18)';
  c.fillStyle = ink;
  const t = cur ? cur.track : null;
  c.font = '20px Dot';
  c.fillText(t ? `♫ ${t.title} · ${t.artist}` : '♫ KAWAII RADIO · ulanmoqda…', 28, 34, LW - 250);
  const pos = audio.currentTime || 0, dur = t ? (t.duration || trackDur()) : 0;
  c.font = '46px VT'; c.textAlign = 'right';
  c.fillStyle = ghost; c.fillText('8:88', LW - 88, 44);
  c.fillStyle = ink; c.fillText(fmt(pos), LW - 88, 44);
  c.font = '22px VT'; c.fillStyle = 'rgba(35,43,27,.45)'; c.fillText('/ ' + fmt(dur), LW - 20, 44);
  c.textAlign = 'left';
  const frac = dur ? Math.min(1, pos / dur) : 0;
  const n = wave.length || 180;
  for (let i = 0; i < n; i++) {
    const x = 27 + i * (WV / n), h = wave[i] || 6;
    c.fillStyle = i / n < frac ? ink : ghost;
    c.fillRect(x, 77 - h / 2, WV / n * 0.62, h);
  }
  c.fillStyle = ink; c.fillRect(27 + frac * WV, 47, 3, 62);
  c.fillStyle = 'rgba(35,43,27,.18)'; c.fillRect(27, 117, WV, 1);
  c.font = '22px VT'; c.fillStyle = ink;
  const foot = t ? `${S.mode === 'pick' ? "O'Z TANLOVIM" : stationName()} · ${t.category.toUpperCase()} · ${t.bpm || '—'} BPM · ENERGIYA ${t.energy || 0}%` : '';
  c.fillText(foot, 29, 141);
  // seek preview: a ghost head and the time it would jump to
  if (lcdHover != null && dur) {
    const hx = 27 + lcdHover * WV;
    c.fillStyle = 'rgba(35,43,27,.45)'; c.fillRect(hx - 1, 50, 2, 56);
    const label = fmt(lcdHover * dur);
    c.font = '22px VT'; const w = c.measureText(label).width + 14;
    const bx = Math.max(27, Math.min(27 + WV - w, hx - w / 2));
    c.fillStyle = ink; c.fillRect(bx, 118, w, 26);
    c.fillStyle = '#c9d4a0'; c.fillText(label, bx + 7, 138);
  }
}

/* ───────────── tuner drum: ticks around a cylinder, turning with the music */
const drum = $('#drum').getContext('2d');
let drumA = 0, drumKick = 0;
function tick(n) { drumKick += n; }
// the tuner drum is a real part: a brushed-aluminium cylinder (graphite anodised in the dark theme) with a printed
// scale, lit from the upper left, turning away at both ends, a red acrylic pointer in front of it
let brush = null;
function brushed() {
  // circumferential brushing: fine horizontal streaks (they run along the turn, so they never move)
  const b = document.createElement('canvas'); b.width = 1000; b.height = 184;
  const g = b.getContext('2d');
  for (let y = 0; y < 184; y++) {
    const a = Math.random();
    g.fillStyle = a > .5 ? `rgba(255,255,255,${(a - .5) * .16})` : `rgba(0,0,0,${(.5 - a) * .2})`;
    g.fillRect(0, y, 1000, 1);
    for (let k = 0; k < 6; k++) { g.fillStyle = `rgba(255,255,255,${Math.random() * .07})`; g.fillRect(Math.random() * 1000, y, 40 + Math.random() * 260, 1); }
  }
  return b;
}
function drawDrum(dt) {
  const c = drum, W = 494, H = 92, R = 239, light = S.theme === 'light';
  brush = brush || brushed();
  drumA += drumKick * 0.02;
  drumKick *= 0.9;
  // body: the cylinder's shading across its width
  const body = c.createLinearGradient(0, 0, W, 0);
  const st = light ? ['#5d636c', '#9aa0a9', '#d9dde2', '#f4f5f7', '#c9ced5', '#8a9099', '#4f545c']
                   : ['#0c0d0f', '#25282d', '#40444b', '#565b63', '#3a3e45', '#212328', '#0b0c0e'];
  [0, .1, .3, .42, .62, .88, 1].forEach((p, i) => body.addColorStop(p, st[i]));
  c.fillStyle = body; c.fillRect(0, 0, W, H);
  c.globalAlpha = light ? .55 : .8; c.drawImage(brush, 0, 0, W, H); c.globalAlpha = 1;
  const deg0 = drumA * 57.3;
  // knurled rims (like a thumbwheel): fine machined ribs every 1.5°, they turn with the drum and squeeze at the ends
  for (const [y0, y1] of [[0, 11], [H - 11, H]]) {
    for (let k = -120; k <= 120; k++) {
      const th = (k * 1.5 - (deg0 % 1.5)) * Math.PI / 180;
      if (Math.abs(th) > 1.45) continue;
      const x = W / 2 + R * Math.sin(th), vis = Math.cos(th), w = Math.max(.4, 1.6 * vis);
      c.fillStyle = `rgba(255,255,255,${(light ? .5 : .22) * vis})`; c.fillRect(x - w, y0, w * .6, y1 - y0);
      c.fillStyle = `rgba(0,0,0,${(light ? .35 : .55) * vis})`; c.fillRect(x, y0, w * .7, y1 - y0);
    }
  }
  // the printed band: a satin ivory/graphite strip between the rims, with a hairline edge on each side
  const band = c.createLinearGradient(0, 0, W, 0);
  const bs = light ? ['rgba(236,231,214,.0)', 'rgba(236,231,214,.85)', 'rgba(246,242,228,.95)', 'rgba(236,231,214,.85)', 'rgba(236,231,214,0)']
                   : ['rgba(22,23,26,0)', 'rgba(22,23,26,.75)', 'rgba(30,31,35,.8)', 'rgba(22,23,26,.75)', 'rgba(22,23,26,0)'];
  [0, .12, .45, .88, 1].forEach((p, i) => band.addColorStop(p, bs[i]));
  c.fillStyle = band; c.fillRect(0, 12, W, H - 24);
  c.fillStyle = light ? 'rgba(120,110,90,.35)' : 'rgba(255,255,255,.08)'; c.fillRect(0, 12, W, 1); c.fillRect(0, H - 13, W, 1);
  // the printed scale: ticks and numbers squeezed by the curvature, fading as they turn away
  const ink = light ? '44,40,34' : '232,226,208';   // warm printer's ink on the band
  const deg = drumA * 57.3, base = Math.floor(deg / 2);
  c.textAlign = 'center'; c.textBaseline = 'alphabetic';
  for (let k = -64; k <= 64; k++) {
    const th = (k * 2 - (deg - base * 2)) * Math.PI / 180;
    if (Math.abs(th) > 1.42) continue;
    const x = W / 2 + R * Math.sin(th), vis = Math.cos(th);
    const n = k + base, long = ((n % 5) + 5) % 5 === 0;
    const a = (0.12 + 0.78 * vis * vis).toFixed(3);
    c.fillStyle = `rgba(${ink},${a})`;
    const w = (long ? 2.2 : 1.1) * (0.35 + 0.65 * vis);
    const top = long ? 16 : 22, bot = long ? 54 : 46;
    c.fillRect(x - w / 2, top, w, bot - top);
    if (long) {
      c.save(); c.translate(x, 76); c.scale(Math.max(.05, vis), 1);
      c.font = '600 15px Barlow, sans-serif';
      const v = (((n / 5) % 100) + 100) % 100;
      if (v % 10 === 0) {
        // every tenth mark: a little pixel heart in KR red instead of a number
        c.fillStyle = `rgba(226,70,60,${(0.2 + 0.75 * vis * vis).toFixed(3)})`;
        const HEART = [' XX XX ', 'XXXXXXX', 'XXXXXXX', ' XXXXX ', '  XXX  ', '   X   '];
        HEART.forEach((row, ry) => [...row].forEach((ch, rx) => { if (ch === 'X') c.fillRect((rx - 3.5) * 1.7, (ry - 6) * 1.7 - 2, 1.7, 1.7); }));
      } else c.fillText(String(v).padStart(2, '0'), 0, 0);   // an odometer-like counter scale
      c.restore();
    }
  }
  c.fillStyle = `rgba(${ink},.32)`; c.fillRect(0, 59, W, 1);
  // the drum turning away into the dark at both ends, and the lip of the window shading top and bottom
  const side = c.createLinearGradient(0, 0, W, 0);
  side.addColorStop(0, 'rgba(0,0,0,.75)'); side.addColorStop(.14, 'rgba(0,0,0,0)'); side.addColorStop(.86, 'rgba(0,0,0,0)'); side.addColorStop(1, 'rgba(0,0,0,.8)');
  c.fillStyle = side; c.fillRect(0, 0, W, H);
  const lip = c.createLinearGradient(0, 0, 0, H);
  lip.addColorStop(0, 'rgba(0,0,0,.6)'); lip.addColorStop(.16, 'rgba(0,0,0,0)'); lip.addColorStop(.84, 'rgba(0,0,0,0)'); lip.addColorStop(1, 'rgba(0,0,0,.45)');
  c.fillStyle = lip; c.fillRect(0, 0, W, H);
  // a soft specular stripe along the axis
  const sp = c.createLinearGradient(130, 0, 230, 0);
  sp.addColorStop(0, 'rgba(255,255,255,0)'); sp.addColorStop(.5, `rgba(255,255,255,${light ? .28 : .1})`); sp.addColorStop(1, 'rgba(255,255,255,0)');
  c.fillStyle = sp; c.fillRect(130, 0, 100, H);
  // the pointer: red acrylic, a shadow on the drum, a bright edge, a little light inside
  const px = W / 2;
  c.fillStyle = 'rgba(0,0,0,.35)'; c.fillRect(px + 2, 8, 6, H - 14);
  const nd = c.createLinearGradient(px - 4, 0, px + 4, 0);
  nd.addColorStop(0, '#7e1a16'); nd.addColorStop(.35, '#d9453a'); nd.addColorStop(.6, '#ec7466'); nd.addColorStop(1, '#8e1d18');
  c.shadowColor = 'rgba(230,60,40,.28)'; c.shadowBlur = 4;
  c.fillStyle = nd; c.beginPath(); c.roundRect(px - 4, 6, 8, H - 12, 3); c.fill();
  c.shadowBlur = 0;
  c.fillStyle = 'rgba(255,225,215,.45)'; c.fillRect(px - 1.5, 9, 1.2, H - 18);
  // the glass over it: one faint diagonal reflection
  const gl = c.createLinearGradient(0, 0, W, H);
  gl.addColorStop(.25, 'rgba(255,255,255,0)'); gl.addColorStop(.32, 'rgba(255,255,255,.05)'); gl.addColorStop(.36, 'rgba(255,255,255,0)');
  c.fillStyle = gl; c.fillRect(0, 0, W, H);
}

/* ───────────── volume knob: wheel or drag; the LED ring and the red dot follow */
let setVolume = function setVolume(v) {
  S.volume = Math.max(0, Math.min(1, v));
  audio.volume = S.volume;
  store.set('volume', S.volume);
  const lit = Math.round(S.volume * 20);
  ring.forEach((r, i) => on(r, i <= lit && S.volume > 0));
  $('#knobDot').style.transform = `rotate(${-135 + S.volume * 270}deg)`;
}
const knobHit = ctl('knob', 1150, 858, 1350, 1058, { cls: 'round noclick' });
knobHit.classList.remove('noclick');
knobHit.querySelector('.lit').remove();
knobHit.addEventListener('wheel', (e) => { e.preventDefault(); setVolume(S.volume - Math.sign(e.deltaY) * 0.05); click(); }, { passive: false });
let dragY = null;
knobHit.addEventListener('mousedown', (e) => { dragY = e.clientY; e.stopPropagation(); });
addEventListener('mouseup', () => { dragY = null; });
addEventListener('mousemove', (e) => { if (dragY == null) return; setVolume(S.volume + (dragY - e.clientY) / 300); dragY = e.clientY; });
setVolume(S.volume);

/* ───────────── level meter: the real signal through an analyser */
let analyser, abuf, vuA = null;
function ensureAnalyser() {
  if (analyser) return;
  try {
    actx = actx || new AudioContext();
    const src = actx.createMediaElementSource(audio);
    analyser = actx.createAnalyser(); analyser.fftSize = 1024;
    src.connect(analyser); analyser.connect(actx.destination);
    // stereo for the VU meters: left and right through their own analysers
    const sp = actx.createChannelSplitter(2);
    src.connect(sp);
    vuA = [0, 1].map((ch) => { const a = actx.createAnalyser(); a.fftSize = 1024; sp.connect(a, ch); return a; });
    abuf = new Float32Array(analyser.fftSize);
    if (actx.state === 'suspended') actx.resume();
  } catch (e) { console.warn('analyser', e); }
}
let level = 0;
let drawMeter = function drawMeter() {
  let v = 0;
  if (analyser && S.playing) {
    analyser.getFloatTimeDomainData(abuf);
    let s = 0; for (let i = 0; i < abuf.length; i++) s += abuf[i] * abuf[i];
    v = Math.min(1, Math.sqrt(s / abuf.length) * 3.2);
  }
  level = v > level ? v : level * 0.9 + v * 0.1;
  // analogue LEDs: the top one glows partly, each fades with a short afterglow instead of snapping off
  const x = level * 18;
  meter.forEach((m, j) => {
    const want = Math.max(0, Math.min(1, x - (17 - j)));
    const lit = m._lit || (m._lit = m.querySelector('.lit'));
    m._b = want > (m._b || 0) ? want : (m._b || 0) * 0.82;
    if (lit) { lit.style.transition = 'none'; lit.style.opacity = m._b < 0.02 ? 0 : (0.25 + 0.75 * m._b).toFixed(3); }
    m.classList.toggle('on', m._b > 0.35);
  });
  drawVU();
}
// VU meters: real ballistics — about 300 ms to rise, a little overshoot, they rest left of -20 when silent
const VU_DB = [[-20, -48], [-10, -28], [-7, -17], [-5, -8], [-3, 3], [-2, 10], [-1, 17], [0, 25], [1, 32], [2, 40], [3, 48]];
const vuAng = (db) => {
  if (db <= -20) return Math.max(-54, -48 - (-20 - db) * 0.4);
  for (let i = 1; i < VU_DB.length; i++) if (db <= VU_DB[i][0]) {
    const [d0, a0] = VU_DB[i - 1], [d1, a1] = VU_DB[i];
    return a0 + (a1 - a0) * (db - d0) / (d1 - d0);
  }
  return 50;
};
const vuEl = [$('#vuL'), $('#vuR')], vuS = [{ a: -54, v: 0 }, { a: -54, v: 0 }];
let vbuf;
function drawVU() {
  vuEl.forEach((el, ch) => {
    let want = -54;
    if (vuA && S.playing) {
      vbuf = vbuf || new Float32Array(vuA[ch].fftSize);
      vuA[ch].getFloatTimeDomainData(vbuf);
      let s2 = 0; for (let i = 0; i < vbuf.length; i++) s2 += vbuf[i] * vbuf[i];
      const rms = Math.sqrt(s2 / vbuf.length) * (S.volume == null ? 1 : 1);
      want = vuAng(20 * Math.log10(Math.max(1e-5, rms)) + 14);
    }
    // a damped spring: the needle has mass
    const st = vuS[ch];
    st.v += (want - st.a) * 0.045; st.v *= 0.78; st.a += st.v;
    el.style.transform = `rotate(${st.a.toFixed(2)}deg)`;
  });
}

/* ───────────── the receipt: printed song info, wound like a scroll */
let paperOut = 0, paperScroll = 0, paperVel = 0, paperGoal = null, printing = null;
let buildReceipt = function buildReceipt(t, animate) {
  const info = t.info || {};
  const d = new Date();
  const energy = Math.round((t.energy || 0) / 100 * 12);
  let h = `<div class="c h1">★ KAWAII RADIO ★</div><div class="c soft">KR-01 · QO'SHIQ TAVSIFI</div><div class="dash"></div>
    <div class="title">${esc(t.title)}</div><div class="artist">${esc(t.artist)}</div>
    <div class="kv">${esc(t.category)} · ${fmt(t.duration)} · ${t.bpm || '—'} BPM</div>
    <div class="bar">ENERGIYA ${Array.from({ length: 12 }, (_, k) => `<i class="${k < energy ? '' : 'o'}"></i>`).join('')} ${t.energy || 0}%</div>
    <div class="dash"></div>`;
  h += `<div>${esc(info.description || "Bu qo'shiq uchun tavsif hali yo'q.")}</div>`;
  for (const s of info.details || []) h += `<div class="sec">${esc(s.title).toUpperCase()}</div><div>${esc(s.text)}</div>`;
  h += `<div class="dash"></div><div class="soft">#${String(Math.abs(hashOf(t.id)) % 10000).padStart(4, '0')}   ${d.toLocaleDateString('uz')}   ${d.toTimeString().slice(0, 5)}</div>`;
  h += `<div class="barcode">${Array.from({ length: 70 }, (_, k) => `<i style="width:${[1.6, 1.6, 3.2, 4.8][(hashOf(t.id + k) >>> 0) % 4]}px"></i>`).join('')}</div>`;
  const p = $('#paper');
  p.innerHTML = `<div class="ink">${h}</div>`;
  paperScroll = 0; paperVel = 0; paperGoal = null;
  if (animate) { paperOut = 0; printing = performance.now(); on(printerLed, true); }
  else { paperOut = 1e6; }
  layoutPaper();
}
function hashOf(s) { let h = 0; for (const c of s) h = (h * 31 + c.charCodeAt(0)) | 0; return h; }
function printReceipt() {
  if (!cur) return;
  on(tavsifKey, true);
  setTimeout(() => on(tavsifKey, false), 160);
  buildReceipt(cur.track, true);
}
function layoutPaper() {
  const p = $('#paper');
  const full = p.scrollHeight;
  const maxScroll = Math.max(0, full - 432);
  paperScroll = Math.max(0, Math.min(maxScroll, paperScroll));
  // a real feed: the paper itself slides out of the head's slot (it prints the receipt bottom-up, so when it is done
  // the header sits at the slot and reads top-down); what is still inside the printer is above the window
  const inside = Math.max(0, full - paperOut);
  p.style.transform = `translate3d(0, ${(-inside - paperScroll).toFixed(2)}px, 0)`;
  p.style.setProperty('--out', full + 'px');
  const lines = Math.max(1, Math.round(full / 21)), line = Math.min(lines, Math.round(paperScroll / 21) + 1);
  $('#qator').textContent = `QATOR ${String(line).padStart(2, '0')}/${String(lines).padStart(2, '0')}`;
}

$('#paperWin').addEventListener('wheel', (e) => {
  e.preventDefault();
  const step = e.deltaMode === 1 ? e.deltaY * 21 : e.deltaY * 0.5;   // a notch ~ 2.5 printed lines
  const max = Math.max(0, $('#paper').scrollHeight - 432);
  paperGoal = Math.max(0, Math.min(max, (paperGoal ?? paperScroll) + step));
}, { passive: false });
function stepPaper(dt) {
  if (printing != null) {
    // a stepper motor: 3 px steps, a printed line (21 px) = 7 steps, then a tiny dwell while the head burns the next one
    const ms = performance.now() - printing;
    const LINE = 7 * 9 + 14;                               // ms per line: steps + dwell
    const ln = Math.floor(ms / LINE), inLine = Math.min(7, Math.floor((ms % LINE) / 9));
    paperOut = ln * 21 + inLine * 3;
    $('#paperWin').classList.toggle('feeding', true);
    if (paperOut >= $('#paper').scrollHeight) { printing = null; paperOut = 1e6; on(printerLed, false); $('#paperWin').classList.remove('feeding'); }
    layoutPaper();
  }
  if (paperGoal != null) {
    const d = paperGoal - paperScroll;
    if (Math.abs(d) < 0.3) { paperScroll = paperGoal; paperGoal = null; }
    else paperScroll += d * (1 - Math.exp(-(dt || 0.016) / 0.11));
    layoutPaper();
  }
}

/* ───────────── chat (read-only without Kawaii ID) */
function chatId() {
  let id = store.get('chat', null);
  if (!id) {
    const r = (n) => Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, '0')).join('');
    id = { uid: 'desk-' + r(8), key: r(24) };
    store.set('chat', id);
  }
  return id;
}
function avatarUrl(a) {
  if (!a) return null;
  if (a.startsWith('k:')) return a.slice(2);
  if (a.startsWith('u:')) { const p = a.split(':'); return `${API}/files/avatars/${p[1]}.jpg?v=${p[2]}`; }
  return null;
}
let msgHtml = function msgHtml(m) {
  const av = avatarUrl(m.avatar);
  const time = new Date(m.ts).toTimeString().slice(0, 5);
  let body;
  if (m.kind === 'sticker') {
    const f = stickers.get(m.body) || (m.body.startsWith('/') || m.body.startsWith('http') ? m.body : `/files/stickers/${m.body}.webp`);
    body = /\.(webm|mp4)$/.test(f) ? `<video class="st" src="${url(f)}" autoplay loop muted playsinline></video>` : `<img class="st" src="${url(f)}">`;
  }
  else if (m.kind === 'track') {
    const t = tracks.get(m.body);
    body = t ? `<div class="song"><img src="${url(t.coverArt)}"><div><div class="t">♪ ${esc(t.title)}</div><div class="n">${esc(t.artist)}</div></div></div>` : `<div class="t">♪ qo'shiq</div>`;
  } else body = `<div class="t">${esc(m.body).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>')}</div>`;
  return `<div class="m" data-id="${m.id}"><div class="av" style="${av ? `background-image:url('${av}')` : ''}"></div><div class="bd"><div class="n" style="color:${nameInk(m.name)}">${esc(m.name)}<span class="ts">${time}</span></div>${body}</div></div>`;
}
// phosphor colours a CRT can show well; a name always gets the same one
const INKS = ['#ffcf7a', '#7fe9ff', '#ff9fd8', '#9dff9a', '#c8b0ff', '#ffa98a'];
function nameInk(n) { let h = 0; for (const c of String(n || '')) h = (h * 31 + c.codePointAt(0)) >>> 0; return INKS[h % INKS.length]; }
function addMsgs(list, replace) {
  const box = $('#chMsgs');
  if (replace) box.innerHTML = '';
  box.insertAdjacentHTML('beforeend', list.map(msgHtml).join(''));
  box.scrollTop = box.scrollHeight;
}
const stickers = new Map();
async function loadStickers() {
  try {
    const j = await (await fetch(API + '/files/stickers/pack.json')).json();
    for (const st of j.stickers || []) stickers.set(st.id, st.file);
  } catch {}
}
let chatWs = null;
function connectChat() {
  let ws;
  try { ws = new WebSocket(API.replace('https', 'wss') + '/chat'); } catch { return; }
  chatWs = ws;
  ws.onopen = () => {
    const id = chatId();
    const nowId = (S.playing && cur && cur.track) ? cur.track.id : null;
    ws.send(JSON.stringify({
      type: 'hello',
      uid: id.uid,
      key: id.key,
      name: kid.acc ? (kid.acc.name || kid.acc.username) : 'Mehmon',
      avatar: kid.acc ? (kid.acc.avatar ? 'k:' + kid.acc.avatar : 'p:0') : 'p:0',
      live: S.mode !== 'pick' && S.playing,
      fg: true,
      card: {
        now: nowId,
        likes: S.liked.size,
        plays: store.get('songs', 0),
        minutes: Math.round(store.get('listened', 0) / 60)
      }
    }));
  };
  ws.onmessage = (e) => {
    let m; try { m = JSON.parse(e.data); } catch { return; }
    if (m.type === 'welcome') {
      addMsgs(m.history || [], true);
      presence(m);
      room(m.room);
      kid.you = m.you || null;
      if (kid.token) kidMe();
      syncListening();
    }
    else if (m.type === 'you') { kid.you = m.you || m; }
    else if (m.type === 'face') { if (m.uid === chatId().uid && kid.you) kid.you.kid = m.kid; }
    else if (m.type === 'error') toast(String(m.text || 'XATO').toUpperCase());
    else if (m.type === 'msg') addMsgs([m.m]);
    else if (m.type === 'presence') presence(m);
    else if (m.type === 'deleted') { const el = document.querySelector(`.m[data-id="${m.id}"]`); if (el) el.remove(); }
    else if (m.type === 'cleared') $('#chMsgs').innerHTML = '';
    else if (m.type === 'room') room(m);
  };
  ws.onclose = () => { if (chatWs === ws) chatWs = null; $('#chOnline').textContent = 'qayta ulanmoqda…'; setTimeout(connectChat, 4000); };
  const ping = setInterval(() => { if (ws.readyState === 1) ws.send('ping'); else if (ws.readyState > 1) clearInterval(ping); }, 25000);
}

let playTimer = null;
function reportPlayTimer(tid) {
  clearTimeout(playTimer);
  playTimer = setTimeout(async () => {
    if (S.playing && !audio.paused && cur && cur.track && cur.track.id === tid) {
      const id = chatId();
      try {
        await invoke('kid_api', {
          path: '/chat/play',
          token: kid.token || '',
          uid: id.uid,
          key: id.key,
          body: JSON.stringify({ uid: id.uid, key: id.key, id: tid })
        });
      } catch {}
    }
  }, 30000);
}

function syncListening() {
  if (!chatWs || chatWs.readyState !== 1) return;
  const nowId = (S.playing && cur && cur.track) ? cur.track.id : null;
  const payload = {
    type: 'profile',
    name: kid.acc ? (kid.acc.name || kid.acc.username) : 'Mehmon',
    avatar: kid.acc ? (kid.acc.avatar ? 'k:' + kid.acc.avatar : 'p:0') : 'p:0',
    card: {
      now: nowId,
      likes: S.liked.size,
      plays: store.get('songs', 0),
      minutes: Math.round(store.get('listened', 0) / 60)
    }
  };
  chatWs.send(JSON.stringify(payload));
  chatWs.send(JSON.stringify({
    type: 'presence',
    live: S.mode !== 'pick' && S.playing,
    viewing: true,
    fg: true
  }));
}

async function syncProgress() {
  if (!kid.token) return;
  const id = chatId();
  const stats = store.get('listeningStats', {});
  const body = JSON.stringify({
    devices: {
      desk: {
        seen: Date.now(),
        stats: stats
      }
    }
  });
  try {
    await invoke('kid_api', {
      path: '/auth/sync',
      token: kid.token,
      uid: id.uid,
      key: id.key,
      body: body
    });
    await kidMe();
  } catch {}
}
function presence(m) {
  if (m.online == null) return;
  $('#chOnline').textContent = `${m.online} onlayn · ${m.live ?? 0} radioda`;
  setListeners(m.live ?? m.online);
}
function room(r) {
  if (!r) return;
  const p = r.pinned || r.pin;
  const box = $('#chPin');
  if (p && (p.body || p.text)) {
    box.hidden = false;
    $('#chPinWho').textContent = 'QADALGAN';
    const txt = (p.body || p.text || '').replace(/\*\*/g, '').replace(/\s+/g, ' ').trim() + (p.name ? '  — ' + p.name : '');
    const run = $('#chPinText');
    run.innerHTML = `<span>${esc(txt)}</span><span>${esc(txt)}</span>`;
    run.style.setProperty('--dur', Math.max(12, txt.length * 0.16) + 's');
  } else box.hidden = true;
}

/* ───────────── Kawaii ID: sign in (a small login window, see kid_login in main.rs), write in the chat */
async function kidMe() {
  if (!kid.token) return;
  const id = chatId();
  try {
    const [st, body] = await invoke('kid_api', { path: '/auth/me', token: kid.token, uid: id.uid, key: id.key });
    if (st === 200) { const j = JSON.parse(body); if (j.ok && j.account) { kid.acc = j.account; store.set('kidAcc', kid.acc); } }
    else if (st === 401) { kid.token = null; kid.acc = null; store.set('kidToken', null); store.set('kidAcc', null); toast('KAWAII ID · QAYTA KIRING'); }
  } catch {}
  composeUi();
  if (screen === 4) fillScreen(4);
}
let kidPoll = null;
async function kidLogin() {
  if (!tauri) { toast('KAWAII ID · ILOVADA'); return; }
  const state = Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, '0')).join('');
  try { await invoke('kid_login', { state }); } catch { toast('KAWAII ID · OCHILMADI'); return; }
  toast('KAWAII ID · KIRISH OYNASI');
  // the login window hands the token over and closes itself; wait while it is open (up to ten minutes)
  clearInterval(kidPoll);
  const until = Date.now() + 600000;
  let seenOpen = false;
  kidPoll = setInterval(async () => {
    const t = await invoke('kid_take');
    if (t) {
      clearInterval(kidPoll);
      kid.token = t; store.set('kidToken', t);
      // links this chat identity to the account (a later welcome does it again)
      await kidMe();
      if (kid.acc) { toast(`SALOM, ${String(kid.acc.name || kid.acc.username).toUpperCase()}`); sfx('up'); }
      else toast('KAWAII ID · XATO, QAYTA URINING');
      return;
    }
    const open = await invoke('kid_open');
    if (open) seenOpen = true;
    if (Date.now() > until || (seenOpen && !open)) clearInterval(kidPoll);
  }, 700);
}
async function kidLogout() {
  const id = chatId();
  if (kid.token) { try { await invoke('kid_api', { path: '/auth/logout', token: kid.token, uid: id.uid, key: id.key }); } catch {} }
  kid.token = null; kid.acc = null; store.set('kidToken', null); store.set('kidAcc', null);
  composeUi(); toast('KAWAII ID · CHIQILDI');
  if (screen === 4) fillScreen(4);
}
function chatSend(kind, body) {
  if (!chatWs || chatWs.readyState !== 1) { toast('CHAT · ULANMOQDA'); return false; }
  chatWs.send(JSON.stringify({ type: 'send', kind, body, nonce: Math.random().toString(36).slice(2) }));
  return true;
}
// the input strip under the tube: a real text field once signed in
function composeUi() {
  const c = $('#chCompose');
  if (kid.acc) {
    if (!$('#chText')) {
      c.innerHTML = '<input id="chText" maxlength="300" spellcheck="false" placeholder="XABAR YOZING">';
      const inp = $('#chText');
      inp.addEventListener('keydown', (e) => {
        e.stopPropagation();
        if (e.key === 'Enter' && inp.value.trim()) { if (chatSend('text', inp.value.trim())) { inp.value = ''; sfx('tick'); } }
        if (e.key === 'Escape') inp.blur();
      });
    }
    c.classList.add('live');
  } else {
    c.innerHTML = '<div class="cin" id="chInput">KAWAII ID BILAN KIRING</div>';
    c.classList.remove('live');
  }
}
$('#chCompose').addEventListener('click', () => { if (!kid.acc) { click(); kidLogin(); } });
// stickers: a tray that slides up over the bottom of the tube
function stickerTray() {
  let t = $('#stTray');
  if (t) { t.remove(); return; }
  t = document.createElement('div'); t.id = 'stTray';
  t.innerHTML = [...stickers.entries()].filter(([, f]) => !/\.(webm|mp4)$/.test(f)).slice(0, 48)
    .map(([id, f]) => `<img data-id="${esc(id)}" src="${url(f)}" loading="lazy">`).join('') || '<div class="note">Stikerlar yuklanmoqda…</div>';
  t.addEventListener('click', (e) => {
    const im = e.target.closest('img');
    if (im && chatSend('sticker', im.dataset.id)) { click(); t.remove(); }
  });
  $('#chat').append(t);
}
composeUi();

/* ───────────── listeners: counter + a row of pixel people in the VFD window */
const LIS_MAX = 20;
$('#lisP').innerHTML = Array.from({ length: LIS_MAX }, () => '<i></i>').join('');
let lisCount = 0;
function setListeners(n) {
  lisCount = Math.max(0, n | 0);
  $('#lisN').textContent = String(Math.min(999, lisCount)).padStart(3, '0');
  [...$('#lisP').children].forEach((p, k) => p.classList.toggle('on', k < lisCount));
}
// lit people bob with the music, each a little out of step
function bobListeners() {
  const ps = $('#lisP').children;
  for (let k = 0; k < Math.min(lisCount, LIS_MAX); k++) {
    const lift = S.playing ? Math.max(0, Math.sin(performance.now() / 160 + k * 1.3)) * level * 4 : 0;
    ps[k].style.transform = `translateY(${(-lift).toFixed(1)}px)`;
  }
}

/* ───────────── screens: a module slides over the lyrics display */
let screen = 0;
function openScreen(i) {
  if (i === screen && i !== 0) i = 0;
  screen = i;
  navKeys.forEach((k) => on(k, k._id === i));
  on(gearKey, false);
  const p = $('#panel');
  if (i === 0 || i === 2) {
    if (!p.hidden) shutter(() => { p.hidden = true; });
    if (i === 2) { $('#chMsgs').scrollTop = 1e9; const c = $('#chat'); c.classList.remove('flash'); void c.offsetWidth; c.classList.add('flash'); }
    return;
  }
  shutter(() => fillScreen(i));
}
function fillScreen(i) {
  const p = $('#panel');
  p.hidden = false;
  const list = $('#pList');
  $('#pSearch').hidden = i !== 1;
  if (i === 1) { $('#pTitle').textContent = 'QIDIRUV'; $('#pSub').textContent = `${tracks.size} ta qo'shiq efirda`; search($('#pInput').value || ''); setTimeout(() => $('#pInput').focus(), 500); }
  if (i === 3) { $('#pTitle').textContent = 'KUTUBXONA'; $('#pSub').textContent = `${S.liked.size} ta sevimli qo'shiq`; rows([...S.liked].map((id) => tracks.get(id)).filter(Boolean), "Hali sevimli qo'shiq yo'q — yurakchani bosing."); }
  if (i === 4) {
    $('#pTitle').textContent = 'PROFIL'; $('#pSub').textContent = 'KR-01 DESK';
    const mins = Math.round(store.get('listened', 0) / 60);
    const a = kid.acc;
    const card = a ? `<div class="kid">
        <div class="kav" style="background-image:url('${esc(a.avatar || '')}')"></div>
        <div class="kin"><b>${esc(a.name || a.username)}</b><span>@${esc(a.username)} · LVL ${a.level ?? '—'}${a.kpUntil && a.kpUntil > Date.now() ? ' · KAWAII PASS' : ''}</span>
          ${a.xpNeeded ? `<div class="kxp"><i style="width:${Math.min(100, Math.round(100 * (a.xpInto || 0) / a.xpNeeded))}%"></i></div><span>${a.xpInto || 0} / ${a.xpNeeded} XP</span>` : ''}</div>
        ${a.rankImg ? `<div class="krk"><img src="${esc(a.rankImg)}" alt=""><em>${esc(a.displayLabel || a.displayRank || a.rank || '')}</em></div>` : ''}
      </div>
      <div class="kbtn" id="kidOut">[ CHIQISH ]</div>`
      : `<div class="kbtn" id="kidIn">[ KAWAII ID BILAN KIRISH ]</div><div class="note">Kirgandan keyin chatda yozish, stikerlar va LEVEL ochiladi.</div>`;
    list.innerHTML = `${card}<div class="stat"><div><b>${mins}</b><span>DAQIQA</span></div><div><b>${S.liked.size}</b><span>SEVIMLI</span></div><div><b>${store.get('songs', 0)}</b><span>QO'SHIQ</span></div></div>`;
  }
}
$('#pClose').addEventListener('click', () => { click(); openScreen(0); });
function rows(list, empty) {
  const box = $('#pList');
  if (!list.length) { box.innerHTML = `<div class="note">${empty}</div>`; return; }
  box.innerHTML = list.slice(0, 80).map((t) => `<div class="row ${cur && cur.track.id === t.id ? 'now' : ''}" data-id="${t.id}"><img loading="lazy" src="${url(t.coverArt)}"><div><b>${esc(t.title)}</b><span>${esc(t.artist)} · ${esc(t.category)}</span></div><em>${fmt(t.duration)}</em></div>`).join('');
}
$('#pList').addEventListener('click', (e) => {
  if (e.target.closest('#kidIn')) { click(); kidLogin(); return; }
  if (e.target.closest('#kidOut')) { click(); kidLogout(); return; }
  const r = e.target.closest('.row');
  if (!r) return;
  click();
  const t = tracks.get(r.dataset.id);
  if (!t) return;
  if (hasKp()) {
    playPick(t);
  } else {
    buildReceipt(t, true);
    toast(cur && cur.track.id === t.id ? 'HOZIR EFIRDA' : "O'Z TANLOVIM · KAWAII PASS KERAK");
  }
});
const norm = (s) => s.toLowerCase().replace(/[\u02bb\u02bc'`\u2019]/g, '');
let searchResults = [];
function search(q) {
  const n = norm(q.trim());
  const all = [...tracks.values()];
  const hit = n ? all.filter((t) => norm(`${t.title} ${t.artist} ${t.category || ''} ${t.info?.description || ''}`).includes(n)) : all.filter((t) => cur && t.id === cur.track.id).concat(all.slice(0, 60));
  searchResults = hit;
  rows(hit, 'Hech narsa topilmadi.');
}
$('#pInput').addEventListener('input', (e) => search(e.target.value));
$('#pInput').addEventListener('keydown', (e) => e.stopPropagation());
// listening minutes & seconds per track, counted while playing
setInterval(() => {
  if (S.playing && !audio.paused && cur && cur.track) {
    store.set('listened', store.get('listened', 0) + 5);
    const stats = store.get('listeningStats', {});
    const tid = cur.track.id;
    stats[tid] = stats[tid] || { seconds: 0, radio: 0 };
    if (S.mode === 'pick') stats[tid].seconds = (stats[tid].seconds || 0) + 5;
    else stats[tid].radio = (stats[tid].radio || 0) + 5;
    store.set('listeningStats', stats);
  }
}, 5000);
// sync progress to Kawaii ID every 5 minutes
setInterval(syncProgress, 300000);
audio.addEventListener('playing', () => {
  if (audio.dataset.counted !== audio.dataset.id) {
    audio.dataset.counted = audio.dataset.id;
    store.set('songs', store.get('songs', 0) + 1);
  }
});


/* ───────────── fixed device sizes (S / M / L), restored at start */
// the window opens at the size it was left at (S the first time); dragging the border resizes it freely
invoke('set_size', { size: String(store.get('winW', 's')) });
document.querySelectorAll('#rz i').forEach((z) => z.addEventListener('mousedown', (e) => {
  if (e.button === 0 && !document.body.classList.contains('full')) { e.preventDefault(); invoke('start_resize', { dir: z.dataset.d }); }
}));
let winT;
addEventListener('resize', () => {
  clearTimeout(winT);
  winT = setTimeout(async () => {
    const full = await invoke('is_maximized');
    document.body.classList.toggle('full', !!full);
    if (!full && innerWidth >= 360 && Math.abs(innerWidth / innerHeight / (16 / 9) - 1) < 0.04) store.set('winW', Math.round(innerWidth));
  }, 400);
});

/* ───────────── settings: the gear opens a module with real switches */
function openSettings() {
  screen = 5;
  navKeys.forEach((k) => on(k, false));
  on(gearKey, true);
  shutter(() => fillSettings());
}
function fillSettings() {
  $('#panel').hidden = false;
  $('#pSearch').hidden = true;
  $('#pTitle').textContent = 'SOZLAMALAR'; $('#pSub').textContent = 'KR-01 DESK · v' + VERSION;
  const seg = (name, items, val) => `<div class="seg" data-set="${name}">${items.map(([k, l]) => `<i data-v="${k}" class="${k === val ? 'on' : ''}">${l}</i>`).join('')}</div>`;
  $('#pList').innerHTML = `
    <div class="set"><div><b>Mavzu</b><span>Kunduzgi bo'r yoki tungi grafit korpus</span></div>${seg('theme', [['light', 'KUN'], ['dark', 'TUN']], S.theme)}</div>
    <div class="set"><div><b>Stansiya</b><span>Ilova ochilganda qaysi efir</span></div>${seg('station', [['jpop', 'J-POP'], ['ost', 'OST'], ['riot', 'RIOT']], S.station)}</div>
    <div class="set"><div><b>Oyna hajmi</b><span>Tez tanlov; chetidan tortib ham o'zgartiriladi</span></div>${seg('size', [['s', 'S'], ['m', 'M'], ['l', 'L'], ['xl', 'XL']], '')}</div>
    <div class="set"><div><b>To'liq ekran</b><span>F11 — kino rejimi kabi</span></div>${seg('full', [['on', 'F11']], '')}</div>`;
}
$('#pList').addEventListener('click', (e) => {
  const i = e.target.closest('.seg i');
  if (!i) return;
  click();
  const name = i.parentElement.dataset.set, v = i.dataset.v;
  if (name === 'theme') { S.theme = v; store.set('theme', v); applyTheme(); }
  if (name === 'station') tuneStation(v);
  if (name === 'full') invoke('window_action', { action: 'fullscreen' });
  if (name === 'size') invoke('set_size', { size: v });
  if (name !== 'full') fillSettings();
});


/* ───────────── polish pass: sharp canvases, window keys, shutter transitions */
function sharpen() {
  const k = Math.min(3, (parseFloat(stage.dataset.scale) || 1) * (devicePixelRatio || 1));
  for (const [cv, w, h] of [[$('#lcd'), LW, 152], [$('#drum'), 494, 92]]) {
    const W = Math.round(w * k), H = Math.round(h * k);
    if (cv.width !== W) { cv.width = W; cv.height = H; cv.getContext('2d').setTransform(k, 0, 0, k, 0, 0); }
  }
}
addEventListener('resize', () => setTimeout(sharpen, 30));
sharpen();

const WIN = [['minimize', 1592], ['maximize', 1628], ['close', 1664]];
WIN.forEach(([a, cx]) => ctl('win_' + a, cx - 16, 42, cx + 16, 74, { cls: 'round win' + (a === 'close' ? ' close' : ''), title: { minimize: "Yig'ish", maximize: 'Kattalashtirish', close: 'Yopish' }[a],
  click: () => invoke('window_action', { action: a }) }));

// the shutter: down (old content goes dark), swap, up (new content powers on with a scan line)
let shutterBusy = Promise.resolve();
let shutter = function shutter(swap) {
  // the display keeps its glass and dot mesh; only the picture on it changes. The old page dims away with a
  // little phosphor flare, the new one is written in by a soft sweep with a glowing head and settles from bright.
  shutterBusy = shutterBusy.then(async () => {
    const layer = () => ($('#panel').hidden ? $('#lyrIn') : $('#panel'));
    const old = layer();
    await old.animate([{ opacity: 1, filter: 'brightness(1)' }, { opacity: 0, filter: 'brightness(1.7) blur(1px)' }],
      { duration: 130, easing: 'cubic-bezier(.5,0,1,1)', fill: 'forwards' }).finished;
    swap();
    // while a page is up the lyrics are switched off entirely, so they never flash through between two pages
    $('#lyrIn').style.visibility = $('#panel').hidden ? '' : 'hidden';
    const nu = layer();
    old.getAnimations().forEach((a) => a.cancel());
    const ease = 'cubic-bezier(.3,.75,.25,1)', dur = 440;
    $('#scan').animate([{ transform: 'translateX(-40px)', opacity: 0 }, { opacity: 1, offset: .12 }, { opacity: 1, offset: .8 }, { transform: 'translateX(752px)', opacity: 0 }],
      { duration: dur, easing: ease });
    await nu.animate([
      { clipPath: 'inset(0 100% 0 0)', filter: 'brightness(2)' },
      { clipPath: 'inset(0 0 0 0)', filter: 'brightness(1.25)', offset: .75 },
      { clipPath: 'inset(0 0 0 0)', filter: 'brightness(1)' }], { duration: dur + 160, easing: ease }).finished;
  });
  return shutterBusy;
}


/* ───────────── feel pass */
// sounds: a short body thump + a bright tick, from filtered noise — no samples needed
function noiseBuf() {
  if (noiseBuf.b) return noiseBuf.b;
  const b = actx.createBuffer(1, actx.sampleRate * 0.4, actx.sampleRate), d = b.getChannelData(0);
  for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
  return (noiseBuf.b = b);
}
function sfx(kind) {
  try {
    actx = actx || new AudioContext();
    const t = actx.currentTime, out = actx.createGain();
    out.gain.value = 0.55;
    out.connect(actx.destination);
    const burst = (f, q, dur, gain, delay = 0) => {
      const s = actx.createBufferSource(); s.buffer = noiseBuf();
      const bp = actx.createBiquadFilter(); bp.type = 'bandpass'; bp.frequency.value = f; bp.Q.value = q;
      const g = actx.createGain(); g.gain.setValueAtTime(gain, t + delay); g.gain.exponentialRampToValueAtTime(0.0001, t + delay + dur);
      s.connect(bp).connect(g).connect(out); s.start(t + delay); s.stop(t + delay + dur + 0.02);
    };
    const thump = (f, dur, gain) => {
      const o = actx.createOscillator(), g = actx.createGain();
      o.frequency.setValueAtTime(f, t); o.frequency.exponentialRampToValueAtTime(f * 0.5, t + dur);
      g.gain.setValueAtTime(gain, t); g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
      o.connect(g).connect(out); o.start(t); o.stop(t + dur + 0.02);
    };
    if (kind === 'down') { thump(150, 0.05, 0.22); burst(2600, 2.5, 0.025, 0.35); }
    else if (kind === 'up') { burst(4200, 3, 0.018, 0.22); }
    else if (kind === 'tick') { burst(5200, 6, 0.012, 0.18); }
    else if (kind === 'shutter') { burst(900, 0.8, 0.32, 0.12); burst(1800, 1.5, 0.28, 0.06, 0.02); thump(70, 0.12, 0.15); }
    else if (kind === 'latch') { thump(95, 0.07, 0.28); burst(3000, 2, 0.03, 0.3); }
    else if (kind === 'print') {
      // the thermal head: a buzzing hum stepping with the paper feed
      const o = actx.createOscillator(), g = actx.createGain(), lp = actx.createBiquadFilter();
      o.type = 'sawtooth'; o.frequency.value = 118; lp.type = 'bandpass'; lp.frequency.value = 1400; lp.Q.value = 1.2;
      g.gain.setValueAtTime(0, t);
      const dur = kindArg || 1.2;
      for (let k = 0; k < dur * 30; k++) { g.gain.setValueAtTime(0.06, t + k / 30); g.gain.setValueAtTime(0.012, t + k / 30 + 0.02); }
      g.gain.setValueAtTime(0, t + dur);
      o.connect(lp).connect(g).connect(out); o.start(t); o.stop(t + dur + 0.05);
    }
    else if (kind === 'tear') { burst(2400, 0.6, 0.22, 0.25); burst(5000, 1, 0.12, 0.12, 0.05); }
  } catch {}
}
let kindArg = 0;
click = () => sfx('up');

// physical keys: sink on press, spring back with a small overshoot on release
document.querySelectorAll('.ctl:not(.led)').forEach((k) => {
  k.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || k.id === 'knob' || k.id === 'play') return;   // the toggle flips, it does not sink
    k.classList.add('press');
    sfx('down');
  });
  const release = () => {
    if (!k.classList.contains('press')) return;
    k.classList.remove('press');
    k.animate([{ transform: 'translateY(1.4px) scale(.992)' }, { transform: 'translateY(-.7px) scale(1.003)' }, { transform: 'translateY(.25px)' }, { transform: 'none' }],
      { duration: 280, easing: 'cubic-bezier(.2,.8,.3,1)' });
  };
  k.addEventListener('pointerup', release);
  k.addEventListener('pointerleave', release);
  k.addEventListener('pointermove', (e) => {
    const r = k.getBoundingClientRect();
    k.style.setProperty('--mx', ((e.clientX - r.left) / r.width * 100).toFixed(1) + '%');
    k.style.setProperty('--my', ((e.clientY - r.top) / r.height * 100).toFixed(1) + '%');
  });
});

// LED colours: every lens blooms in its own light
meter.forEach((m, j) => m.style.setProperty('--g', j < 2 ? 'rgba(255,70,40,.7)' : j < 5 ? 'rgba(255,170,64,.65)' : 'rgba(70,230,120,.6)'));
ring.forEach((r, i) => r.style.setProperty('--g', i >= 18 ? 'rgba(255,70,40,.7)' : 'rgba(255,175,70,.6)'));
printerLed.style.setProperty('--g', 'rgba(80,240,160,.75)');

// volume ticks sound like a detented knob
const _setVol = setVolume;
let lastStep = Math.round(S.volume * 20);
setVolume = (v) => { _setVol(v); const st = Math.round(S.volume * 20); if (st !== lastStep) { lastStep = st; sfx('tick'); } };

// glass follows the pointer a little (parallax of the reflections)
stage.addEventListener('pointermove', (e) => {
  const r = stage.getBoundingClientRect();
  stage.style.setProperty('--gx', (((e.clientX - r.left) / r.width) * 2 - 1).toFixed(3));
  stage.style.setProperty('--gy', (((e.clientY - r.top) / r.height) * 2 - 1).toFixed(3));
});

// the cover's colour: a vivid average, used for the light around the window
function coverColor(src) {
  const img = new Image();
  img.crossOrigin = 'anonymous';
  img.onload = () => {
    try {
      const c = document.createElement('canvas'); c.width = c.height = 24;
      const g = c.getContext('2d'); g.drawImage(img, 0, 0, 24, 24);
      const d = g.getImageData(0, 0, 24, 24).data;
      let r = 0, gg = 0, b = 0, w = 0;
      for (let i = 0; i < d.length; i += 4) {
        const mx = Math.max(d[i], d[i + 1], d[i + 2]), mn = Math.min(d[i], d[i + 1], d[i + 2]);
        const sat = mx ? (mx - mn) / mx : 0, k = sat * sat * (mx / 255) + 0.02;
        r += d[i] * k; gg += d[i + 1] * k; b += d[i + 2] * k; w += k;
      }
      r /= w; gg /= w; b /= w;
      const mx = Math.max(r, gg, b, 1), f = 255 / mx;
      stage.style.setProperty('--cv', `rgba(${Math.round(r * f)},${Math.round(gg * f)},${Math.round(b * f)},.42)`);
    } catch {}
  };
  img.src = src;
}
const _onTrack = onTrack;
onTrack = (t) => { _onTrack(t); coverColor(url(t.coverArt)); };

// shutter and its sounds
const _shutter = shutter;
shutter = (swap) => { sfx('tick'); return _shutter(() => { sfx('up'); swap(); }); };

// printing: buzz, jitter, the head LED blinks; double click tears the receipt off
const _build = buildReceipt;
buildReceipt = (t, animate) => {
  $('#paper').classList.remove('tear');
  $('#paperEmpty').hidden = true;
  _build(t, animate);
  if (animate) {
    const p = $('#paper');
    const dur = p.scrollHeight / 21 * 0.077;
    kindArg = dur; sfx('print');
    const pw = $('#paperWin'); pw.classList.add('printing');   // the motor's buzz shakes the printer, not the paper's feed
    let blink = setInterval(() => on(printerLed, !printerLed.classList.contains('on')), 120);
    setTimeout(() => { pw.classList.remove('printing'); clearInterval(blink); on(printerLed, true); setTimeout(() => on(printerLed, false), 600); }, dur * 1000);
  }
};
$('#paperWin').addEventListener('dblclick', () => {
  const p = $('#paper');
  if (p.classList.contains('tear') || !p.innerHTML) return;
  sfx('tear');
  p.classList.add('tear');
  setTimeout(() => { p.innerHTML = ''; p.classList.remove('tear'); $('#paperEmpty').hidden = false; $('#qator').textContent = 'QATOR --/--'; }, 700);
});

// guests without a picture: a coloured disc with their initial
const GLYPH = ['#cdbbff', '#b5efd3', '#ffcfa6', '#ff9aa8', '#a6d8ff', '#ffe08a'];
const _msgHtml = msgHtml;
msgHtml = (m) => {
  let h = _msgHtml(m);
  if (!avatarUrl(m.avatar)) {
    const n = parseInt((m.avatar || 'p:0').slice(2)) || 0;
    h = h.replace('<div class="av" style="">', `<div class="av g" style="background:${GLYPH[n % GLYPH.length]}">${esc((m.name || '?').trim()[0] || '?').toUpperCase()}</div>`);
  }
  return h;
};

// power-on: dark, then the lights come up while the LCD boots
let bootUntil = performance.now() + 1700;
$('#power').classList.add('boot');
const _drawLcd = drawLcd;
drawLcd = () => {
  const now = performance.now();
  if (now < bootUntil) {
    const c = lcd, k = 1 - (bootUntil - now) / 1700;
    c.clearRect(0, 0, 5000, 1000);
    c.fillStyle = '#232b1b';
    c.font = '46px VT'; c.fillText('KR-01 DESK', 28, 70);
    c.font = '22px VT'; c.fillText('KAWAII RADIO · ISHGA TUSHMOQDA', 30, 104);
    const n = 40;
    for (let i = 0; i < n; i++) { c.fillStyle = i / n < k ? '#232b1b' : 'rgba(35,43,27,.16)'; c.fillRect(30 + i * (LW - 60) / n, 120, (LW - 60) / n - 5, 10); }
    return;
  }
  _drawLcd();
};
const _drawMeter = drawMeter;
drawMeter = () => {
  const now = performance.now();
  if (now < bootUntil) {
    const k = 1 - (bootUntil - now) / 1700, n = Math.round(Math.sin(k * Math.PI) * 18);
    meter.forEach((m, j) => on(m, 17 - j < n)); drawVU();
    ring.forEach((r, i) => on(r, i < Math.round(Math.sin(k * Math.PI) * 21)));
    return;
  }
  if (!drawMeter.booted) { drawMeter.booted = true; setVolume(S.volume); }
  _drawMeter();
};

/* ───────────── no lyrics: the display becomes a dot-matrix spectrum of the real signal */
const viz = $('#viz').getContext('2d');
let fbuf, peaks = new Float32Array(48);
function drawViz() {
  const show = cur && !activeLines().length;
  $('#viz').style.opacity = show ? 1 : 0;
  if (!show) return;
  const k = Math.min(3, (parseFloat(stage.dataset.scale) || 1) * (devicePixelRatio || 1));
  const cv = $('#viz');
  if (cv.width !== Math.round(700 * k)) { cv.width = Math.round(700 * k); cv.height = Math.round(300 * k); viz.setTransform(k, 0, 0, k, 0, 0); }
  viz.clearRect(0, 0, 700, 300);
  const cols = 64, rows = 30, cw = 700 / cols, rh = 300 / rows;
  let bins = null;
  if (analyser && S.playing) { fbuf = fbuf || new Uint8Array(analyser.frequencyBinCount); analyser.getByteFrequencyData(fbuf); bins = fbuf; }
  for (let c = 0; c < cols; c++) {
    let v = 0;
    if (bins) {
      const a = Math.floor(Math.pow(c / cols, 1.8) * 300) + 1, b = Math.floor(Math.pow((c + 1) / cols, 1.8) * 300) + 2;
      for (let i = a; i < b; i++) v = Math.max(v, bins[i]);
      v /= 255;
    } else v = 0.08 + 0.05 * Math.sin(performance.now() / 600 + c * 0.4);
    const h = Math.round(v * rows);
    peaks[c] = Math.max(peaks[c] - 0.25, h);
    for (let r = 0; r < rows; r++) {
      const lit = r < h, pk = r === Math.round(peaks[c]);
      // round pixels like a real dot-matrix VFD: unlit dots stay faintly visible behind the glass
      viz.fillStyle = lit ? (r > rows * 0.82 ? '#eaffb0' : `rgba(178,206,104,${(0.55 + 0.45 * r / rows).toFixed(2)})`) : pk ? 'rgba(214,240,140,.8)' : 'rgba(120,140,70,.07)';
      viz.beginPath(); viz.arc(c * cw + cw / 2, 300 - (r + 0.5) * rh, Math.min(cw, rh) * 0.36, 0, 6.2832); viz.fill();
    }
  }
}


/* ───────────── LCD segment shadow, retune glitch, shortcuts */
let retuneUntil = 0;
const _drawLcd2 = drawLcd;
drawLcd = () => {
  const c = lcd;
  c.save();
  c.shadowColor = 'rgba(35,43,27,.22)'; c.shadowOffsetX = 1.6; c.shadowOffsetY = 1.6; c.shadowBlur = 0;
  _drawLcd2();
  c.restore();
  const now = performance.now();
  if (now < retuneUntil) {
    // static while the tuner finds the new station
    const k = (retuneUntil - now) / 450;
    for (let i = 0; i < 260 * k; i++) {
      c.fillStyle = `rgba(35,43,27,${(Math.random() * 0.5).toFixed(2)})`;
      c.fillRect(Math.random() * LW, Math.random() * 152, 2 + Math.random() * 10, 2);
    }
    c.fillStyle = `rgba(207,216,170,${(0.5 * k).toFixed(2)})`;
    c.fillRect(0, (now / 3) % 152, LW, 6);
  }
};
const _onTrack2 = onTrack;
onTrack = (t) => { if (tv.dataset.ready) { retuneUntil = performance.now() + 450; tick(40); } _onTrack2(t); };

addEventListener('keydown', (e) => {
  if (document.activeElement && document.activeElement.tagName === 'INPUT') return;
  if ((e.ctrlKey || e.metaKey) && e.code === 'KeyF') { e.preventDefault(); openScreen(1); }
  else if (e.code === 'Digit1') tuneStation('jpop');
  else if (e.code === 'Digit2') tuneStation('ost');
  else if (e.code === 'Digit3') tuneStation('riot');
  else if (e.code === 'ArrowUp') { e.preventDefault(); setVolume(S.volume + 0.05); }
  else if (e.code === 'ArrowDown') { e.preventDefault(); setVolume(S.volume - 0.05); }
  else if (e.code === 'KeyL') toggleLike();
  else if (e.code === 'KeyT') printReceipt();
  else if (e.code === 'KeyM') { audio.muted = !audio.muted; toast(audio.muted ? 'OVOZ O\'CHIQ' : 'OVOZ YONIQ'); }
});

/* ───────────── frame loop */
let lastT = performance.now();
function frame(now) {
  const dt = Math.min(0.1, (now - lastT) / 1000);
  lastT = now;
  drawLcd();
  drawDrum(dt);
  drawMeter();
  renderLyrics(false);
  drawViz();
  bobListeners();
  stepPaper(dt);
  requestAnimationFrame(frame);
}

/* ───────────── start */
(async function start() {
  stationUi();
  requestAnimationFrame(frame);
  if (kid.token) kidMe();
  try {
    await loadTracks();
    await loadSchedule(S.station);
    tune(true);
  } catch (e) {
    toast('INTERNET YO\'Q · QAYTA URINILMOQDA');
    setTimeout(start, 5000);
    return;
  }
  await loadStickers();
  connectChat();
})();
