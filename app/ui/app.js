/* KR-01 DESK — the live layer on top of the rendered device.
 * Coordinates are the 1920 x 1080 design ones (design/blend/main.py); #stage is scaled to the window. */
'use strict';
const API = 'https://radio.kawaii.uz';
const VERSION = '0.1.0';
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
  lang: store.get('lang', 'rom'),
  liked: new Set(store.get('liked', [])),
  playing: false,
};

/* ───────────── scale the device into the window */
function fit() {
  const s = Math.min(innerWidth / 1920, innerHeight / 1080);
  stage.style.transform = `translate(${(innerWidth - 1920 * s) / 2}px, ${(innerHeight - 1080 * s) / 2}px) scale(${s})`;
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
function click() {
  try {
    actx = actx || new AudioContext();
    const t = actx.currentTime, o = actx.createOscillator(), g = actx.createGain();
    o.type = 'square'; o.frequency.setValueAtTime(1800, t); o.frequency.exponentialRampToValueAtTime(400, t + 0.03);
    g.gain.setValueAtTime(0.05, t); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.04);
    o.connect(g).connect(actx.destination); o.start(t); o.stop(t + 0.05);
  } catch {}
}

// nav
const NAV = ['ASOSIY', 'QIDIRUV', 'CHAT', 'KUTUBXONA', 'PROFIL'];
const navKeys = NAV.map((n, i) => ctl('nav' + i, 20, 22 + i * 96, 108, 118 + i * 96, {
  title: n, click: () => openScreen(i),
}));
on(navKeys[0], true);
const gearKey = ctl('gear', 20, 978, 108, 1058, { title: 'Sozlamalar', click: () => (screen === 5 ? openScreen(0) : openSettings()) });
const meter = Array.from({ length: 18 }, (_, j) => led('m' + j, 64, 548 + j * 22, 10));

// band selector (RADIO / O'Z TANLOVIM)
const slider = ctl('slider', 836, 34, 1368, 102, { title: "Radio / O'z tanlovim", click: () => toast("O'Z TANLOVIM · KAWAII PASS BILAN") });

// stations
const STATIONS = [['jpop', 'J-POP FM'], ['ost', 'OST FM'], ['riot', 'RIOT FM']];
const stKeys = STATIONS.map(([key, freq], i) => ctl('st' + i, 152 + i * 216, 112, 360 + i * 216, 178, {
  title: freq, click: () => tuneStation(key),
}));
const onAir = led('onair', 1102, 145, 11);

// language keys of the lyrics display
const LANGS = [['uz', 1200, 1244], ['rom', 1250, 1302], ['jp', 1308, 1352]];
const langKeys = LANGS.map(([k, x0, x1]) => ctl('lang' + k, x0 - 2, 202, x1 + 2, 232, { click: () => { S.lang = k; store.set('lang', k); langUi(); renderLyrics(true); } }));
function langUi() { LANGS.forEach(([k], i) => on(langKeys[i], S.lang === k)); }

// deck
const playKey = ctl('play', 126, 852, 246, 972, { cls: 'round', title: 'Ijro / pauza', click: () => togglePlay() });
const heartKey = ctl('heart', 770, 852, 890, 972, { cls: 'round', title: 'Sevimli', click: () => toggleLike() });
const tavsifKey = ctl('tavsif', 122, 970, 450, 1064, { title: 'Tavsif', click: () => printReceipt() });
const saveKey = ctl('saqlash', 448, 970, 770, 1064, { title: 'Saqlash', click: () => toast('SAQLASH · TEZ ORADA') });
ctl('prev', 1030, 856, 1102, 960, { title: 'Oldingi', click: () => toast('JONLI EFIR · ORQAGA YO\'Q') });
ctl('next', 1030, 956, 1102, 1062, { title: 'Keyingi', click: () => toast('JONLI EFIR · OLDINGA YO\'Q') });
const ring = Array.from({ length: 21 }, (_, i) => {
  const a = (225 - i * 13.5) * Math.PI / 180;
  return led('r' + i, 1250 + Math.cos(a) * 86, 958 - Math.sin(a) * 86, 9);
});
const printerLed = led('pled', 1866, 50, 9);

/* ───────────── window keys and dragging by the chassis */
document.querySelectorAll('#winkeys i').forEach((k) => k.addEventListener('click', () => { click(); invoke('window_action', { action: k.dataset.win }); }));
stage.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  const t = e.target;
  if (t === stage || t.id === 'frame') invoke('start_drag');
});
stage.addEventListener('dblclick', (e) => { if (e.target === stage) invoke('window_action', { action: 'maximize' }); });
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
  if (st !== S.station || !schedule.slots.length) {
    S.station = st; store.set('station', st);
    stationUi();
    await loadSchedule(st);
  }
  tune(true);
}
function stationUi() {
  STATIONS.forEach(([k], i) => on(stKeys[i], S.station === k));
}

function tune(force) {
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
  if (!cur) return;
  const at = (liveNow() - cur.slot.start) / 1000;
  const go = () => { try { audio.currentTime = Math.max(0, at); } catch {} };
  if (audio.readyState >= 1) go(); else audio.addEventListener('loadedmetadata', go, { once: true });
}

function togglePlay() {
  S.playing = !S.playing;
  on(playKey, S.playing);
  if (S.playing) { tune(true); audio.play().catch(() => {}); ensureAnalyser(); }
  else audio.pause();
}
audio.addEventListener('ended', () => setTimeout(() => tune(true), 200));

// keep in step with the broadcast; roll over to the next song; refresh the timetable when it runs short
setInterval(() => {
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
function onTrack(t) {
  trivision(url(t.coverArt));
  $('#lyTitle').textContent = t.title;
  $('#lyArtist').textContent = t.artist;
  parseLyrics(t);
  renderLyrics(true);
  wave = waveOf(t.id);
  on(heartKey, S.liked.has(t.id));
  buildReceipt(t, false);
  if ('mediaSession' in navigator) {
    navigator.mediaSession.metadata = new MediaMetadata({ title: t.title, artist: t.artist, album: 'Kawaii Radio · ' + stationName(), artwork: [{ src: url(t.coverArt), sizes: '512x512' }] });
  }
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

/* ───────────── trivision: 24 three-sided slats, a wave of 120° turns */
const tv = $('#tv');
const NS = 24, TW = 436, SW = TW / NS, AP = SW / (2 * Math.tan(Math.PI / 3));
let tvStep = 0;
const slats = [];
for (let i = 0; i < NS; i++) {
  const s = document.createElement('div');
  s.className = 'slat';
  s.style.transitionDelay = (i * 22) + 'ms';
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
    if (force || lastIdx !== -1) box.innerHTML = '<div class="none">Bu qo\'shiq uchun matn hali yo\'q</div>';
    lastIdx = -1;
    $('#lyRail').innerHTML = '';
    return;
  }
  let i = L.findIndex((l, k) => t >= l.t && (k === L.length - 1 || t < L[k + 1].t));
  if (i < 0) i = t < L[0].t ? -1 : L.length - 1;
  if (force || i !== lastIdx) {
    lastIdx = i;
    const rows = [];
    const at = (k, top, size, color, cls = '') => {
      if (k < 0 || k >= L.length) return;
      rows.push(`<div class="ln ${cls}" style="top:${top}px;font-size:${size}px;color:${color}">${cls === 'now' ? `<span class="k">${esc(L[k].s)}</span>` : esc(L[k].s)}</div>`);
    };
    at(i - 2, 6, 24, '#3f4730'); at(i - 1, 44, 28, '#5f6b44');
    if (i >= 0) at(i, 104, 52, '#e6ff9a', 'now'); else rows.push(`<div class="ln" style="top:104px;font-size:40px;color:#5f6b44">♪ ♪ ♪</div>`);
    // translation under the sung line
    if (i >= 0 && S.lang !== 'uz' && lines.uz.length) {
      const tr = lines.uz.reduce((b, l) => (l.t <= L[i].t + 0.5 ? l : b), null);
      if (tr) rows.push(`<div class="tr" style="top:${L[i].s.length > 34 ? 214 : 166}px">${esc(tr.s)}</div>`);
    }
    const nb = L[i] && L[i].s.length > 34 ? 250 : 216;
    at(i + 1, nb, 30, '#6f7c4c'); at(i + 2, nb + 42, 26, '#4f5838'); at(i + 3, nb + 80, 22, '#373e28');
    box.innerHTML = rows.join('');
    // rail
    const n = Math.min(14, L.length), pos = Math.max(0, Math.round((i / Math.max(1, L.length - 1)) * (n - 1)));
    $('#lyRail').innerHTML = Array.from({ length: n }, (_, k) => `<i class="${k === pos ? 'on' : ''}" style="top:${k * 22}px"></i>`).join('');
  }
  // karaoke fill of the sung line
  if (i >= 0) {
    const a = L[i].t, b = i + 1 < L.length ? L[i + 1].t : a + 5;
    const p = Math.max(0, Math.min(1, (t - a) / Math.max(0.5, b - a)));
    const k = box.querySelector('.now .k');
    if (k) k.style.setProperty('--p', (p * 100).toFixed(1) + '%');
  }
}
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

/* ───────────── the big LCD: title, time, waveform, footer */
const lcd = $('#lcd').getContext('2d');
let wave = [];
function waveOf(id) {
  let h = 2166136261;
  for (const c of id) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  const rnd = () => ((h = Math.imul(h ^ (h >>> 15), 2246822507) ^ Math.imul(h ^ (h >>> 13), 3266489909)) >>> 0) / 4294967296;
  return Array.from({ length: 180 }, (_, i) => Math.min(54, 8 + Math.abs(Math.sin(i * 0.43 + rnd()) * 30 + Math.sin(i * 0.13) * 16 + rnd() * 10)));
}
const fmt = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
function drawLcd() {
  const c = lcd, W = 1254, H = 152;
  c.clearRect(0, 0, W, H);
  const ink = '#232b1b', ghost = 'rgba(35,43,27,.18)';
  c.fillStyle = ink;
  const t = cur ? cur.track : null;
  c.font = '20px Dot';
  c.fillText(t ? `♫ ${t.title} · ${t.artist}` : '♫ KAWAII RADIO · ulanmoqda…', 28, 34);
  const pos = audio.currentTime || 0, dur = t ? t.duration : 0;
  c.font = '46px VT'; c.textAlign = 'right';
  c.fillStyle = ghost; c.fillText('8:88', 1166, 44);
  c.fillStyle = ink; c.fillText(fmt(pos), 1166, 44);
  c.font = '22px VT'; c.fillStyle = 'rgba(35,43,27,.45)'; c.fillText('/ ' + fmt(dur), 1234, 44);
  c.textAlign = 'left';
  const frac = dur ? Math.min(1, pos / dur) : 0;
  const n = wave.length || 180;
  for (let i = 0; i < n; i++) {
    const x = 27 + i * (1200 / n), h = wave[i] || 6;
    c.fillStyle = i / n < frac ? ink : ghost;
    c.fillRect(x, 77 - h / 2, 3.8, h);
  }
  c.fillStyle = ink; c.fillRect(27 + frac * 1200, 47, 3, 62);
  c.fillStyle = 'rgba(35,43,27,.18)'; c.fillRect(27, 117, 1200, 1);
  c.font = '22px VT'; c.fillStyle = ink;
  const L = S.lang;
  const foot = `UZ ${L === 'uz' ? '●' : '○'}   ROM ${L === 'rom' ? '●' : '○'}   JP ${L === 'jp' ? '●' : '○'}        ${t ? `${t.category.toUpperCase()} · ${t.bpm || '—'} BPM · ENERGIYA ${t.energy || 0}%` : ''}`;
  c.fillText(foot, 29, 141);
  c.textAlign = 'right'; c.fillText(S.playing ? 'ON AIR ●' : 'ON AIR ○', 1227, 141); c.textAlign = 'left';
}

/* ───────────── tuner drum: ticks around a cylinder, turning with the music */
const drum = $('#drum').getContext('2d');
let drumA = 0, drumKick = 0;
function tick(n) { drumKick += n; }
function drawDrum(dt) {
  const c = drum, W = 500, H = 92, R = 242;
  c.clearRect(0, 0, W, H);
  drumA += dt * (S.playing ? 0.08 : 0) + drumKick * 0.02;
  drumKick *= 0.9;
  const ink = S.theme === 'light' ? 'rgba(55,65,81,' : 'rgba(225,228,234,';
  for (let k = -60; k <= 60; k++) {
    const th = (k * 2 + (drumA * 57.3) % 2) * Math.PI / 180;
    if (Math.abs(th) > 1.45) continue;
    const x = W / 2 + R * Math.sin(th);
    const vis = Math.cos(th);
    const long = ((k + Math.floor(drumA * 57.3 / 2)) % 5 + 5) % 5 === 0;
    c.fillStyle = ink + (0.15 + 0.55 * vis).toFixed(2) + ')';
    const w = (long ? 2.4 : 1.2) * (0.4 + 0.6 * vis);
    const h = long ? 50 : 32;
    c.fillRect(x - w / 2, H / 2 - h / 2, w, h);
  }
}

/* ───────────── volume knob: wheel or drag; the LED ring and the red dot follow */
function setVolume(v) {
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
let analyser, abuf;
function ensureAnalyser() {
  if (analyser) return;
  try {
    actx = actx || new AudioContext();
    const src = actx.createMediaElementSource(audio);
    analyser = actx.createAnalyser(); analyser.fftSize = 1024;
    src.connect(analyser); analyser.connect(actx.destination);
    abuf = new Float32Array(analyser.fftSize);
    if (actx.state === 'suspended') actx.resume();
  } catch (e) { console.warn('analyser', e); }
}
let level = 0;
function drawMeter() {
  let v = 0;
  if (analyser && S.playing) {
    analyser.getFloatTimeDomainData(abuf);
    let s = 0; for (let i = 0; i < abuf.length; i++) s += abuf[i] * abuf[i];
    v = Math.min(1, Math.sqrt(s / abuf.length) * 3.2);
  }
  level = v > level ? v : level * 0.9 + v * 0.1;
  const n = Math.round(level * 18);
  meter.forEach((m, j) => on(m, 17 - j < n));
}

/* ───────────── the receipt: printed song info, wound like a scroll */
let paperOut = 0, paperScroll = 0, paperVel = 0, printing = null;
function buildReceipt(t, animate) {
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
  p.innerHTML = h;
  paperScroll = 0; paperVel = 0;
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
  const maxScroll = Math.max(0, full - 424);
  paperScroll = Math.max(0, Math.min(maxScroll, paperScroll));
  p.style.top = -paperScroll + 'px';
  p.style.setProperty('--out', Math.min(full, paperOut + paperScroll) + 'px');
  const lines = Math.max(1, Math.round(full / 21)), line = Math.min(lines, Math.round(paperScroll / 21) + 1);
  $('#qator').textContent = `QATOR ${String(line).padStart(2, '0')}/${String(lines).padStart(2, '0')}`;
}
$('#paperWin').addEventListener('wheel', (e) => { e.preventDefault(); paperVel += e.deltaY * 0.35; }, { passive: false });
function stepPaper(dt) {
  if (printing != null) {
    // the head feeds in little steps
    const t = (performance.now() - printing) / 1000;
    const steps = Math.floor(t * 30);
    paperOut = steps * 7;
    if (paperOut > $('#paper').scrollHeight) { printing = null; paperOut = 1e6; on(printerLed, false); }
    layoutPaper();
  }
  if (Math.abs(paperVel) > 0.1) { paperScroll += paperVel; paperVel *= 0.86; layoutPaper(); }
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
function msgHtml(m) {
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
  return `<div class="m" data-id="${m.id}"><div class="av" style="${av ? `background-image:url('${av}')` : ''}"></div><div class="bubble"><div class="n">${esc(m.name)}</div>${body}<div class="ts">${time}</div></div></div>`;
}
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
function connectChat() {
  let ws;
  try { ws = new WebSocket(API.replace('https', 'wss') + '/chat'); } catch { return; }
  ws.onopen = () => {
    const id = chatId();
    ws.send(JSON.stringify({ type: 'hello', uid: id.uid, key: id.key, name: 'Mehmon', avatar: 'p:0', live: true, fg: true }));
  };
  ws.onmessage = (e) => {
    let m; try { m = JSON.parse(e.data); } catch { return; }
    if (m.type === 'welcome') { addMsgs(m.history || [], true); presence(m); room(m.room); }
    else if (m.type === 'msg') addMsgs([m.m]);
    else if (m.type === 'presence') presence(m);
    else if (m.type === 'deleted') { const el = document.querySelector(`.m[data-id="${m.id}"]`); if (el) el.remove(); }
    else if (m.type === 'cleared') $('#chMsgs').innerHTML = '';
    else if (m.type === 'room') room(m);
  };
  ws.onclose = () => { $('#chOnline').textContent = 'qayta ulanmoqda…'; setTimeout(connectChat, 4000); };
  setInterval(() => { if (ws.readyState === 1) ws.send(JSON.stringify({ type: 'ping' })); }, 25000);
}
function presence(m) {
  if (m.online == null) return;
  $('#chOnline').textContent = `${m.online} onlayn · ${m.live ?? 0} radioda`;
  $('#listeners').textContent = `${m.live ?? m.online} TINGLOVCHI`;
}
function room(r) {
  if (!r) return;
  const p = r.pinned || r.pin;
  const box = $('#chPin');
  if (p && (p.body || p.text)) {
    box.hidden = false;
    $('#chPinWho').textContent = 'QADALGAN · ' + (p.name || '').toUpperCase();
    $('#chPinText').textContent = (p.body || p.text || '').replace(/\*\*/g, '');
  } else box.hidden = true;
}

/* ───────────── screens: a module slides over the lyrics display */
let screen = 0;
function openScreen(i) {
  if (i === screen && i !== 0) i = 0;
  screen = i;
  navKeys.forEach((k, j) => on(k, j === i));
  on(gearKey, false);
  const p = $('#panel');
  if (i === 0 || i === 2) {
    if (!p.hidden) { p.classList.add('closing'); setTimeout(() => { p.hidden = true; p.classList.remove('closing'); }, 280); }
    if (i === 2) { $('#chMsgs').scrollTop = 1e9; toast("CHAT · O'NG TOMONDA"); }
    return;
  }
  p.hidden = false; p.classList.remove('closing');
  p.style.animation = 'none'; void p.offsetWidth; p.style.animation = '';
  const list = $('#pList');
  list.style.animation = 'none'; void list.offsetWidth; list.style.animation = '';
  $('#pSearch').hidden = i !== 1;
  if (i === 1) { $('#pTitle').textContent = 'QIDIRUV'; $('#pSub').textContent = `${tracks.size} ta qo'shiq efirda`; search($('#pInput').value || ''); setTimeout(() => $('#pInput').focus(), 300); }
  if (i === 3) { $('#pTitle').textContent = 'KUTUBXONA'; $('#pSub').textContent = `${S.liked.size} ta sevimli qo'shiq`; rows([...S.liked].map((id) => tracks.get(id)).filter(Boolean), "Hali sevimli qo'shiq yo'q — yurakchani bosing."); }
  if (i === 4) {
    $('#pTitle').textContent = 'PROFIL'; $('#pSub').textContent = 'KR-01 DESK';
    const mins = Math.round(store.get('listened', 0) / 60);
    list.innerHTML = `<div class="stat"><div><b>${mins}</b><span>DAQIQA</span></div><div><b>${S.liked.size}</b><span>SEVIMLI</span></div><div><b>${store.get('songs', 0)}</b><span>QO'SHIQ</span></div></div>
      <div class="note">Kawaii ID bilan kirish, LEVEL, sezon RANKI va chatda yozish — kompyuter ilovasiga tez orada keladi.</div>`;
  }
}
$('#pClose').addEventListener('click', () => { click(); openScreen(0); });
function rows(list, empty) {
  const box = $('#pList');
  if (!list.length) { box.innerHTML = `<div class="note">${empty}</div>`; return; }
  box.innerHTML = list.slice(0, 80).map((t) => `<div class="row ${cur && cur.track.id === t.id ? 'now' : ''}" data-id="${t.id}"><img loading="lazy" src="${url(t.coverArt)}"><div><b>${esc(t.title)}</b><span>${esc(t.artist)} · ${esc(t.category)}</span></div><em>${fmt(t.duration)}</em></div>`).join('');
}
$('#pList').addEventListener('click', (e) => {
  const r = e.target.closest('.row');
  if (!r) return;
  click();
  const t = tracks.get(r.dataset.id);
  if (!t) return;
  buildReceipt(t, true);
  toast(cur && cur.track.id === t.id ? 'HOZIR EFIRDA' : 'TAVSIF CHOP ETILDI');
});
const norm = (s) => s.toLowerCase().replace(/[\u02bb\u02bc'`\u2019]/g, '');
function search(q) {
  const n = norm(q.trim());
  const all = [...tracks.values()];
  const hit = n ? all.filter((t) => norm(t.title + ' ' + t.artist).includes(n)) : all.filter((t) => cur && t.id === cur.track.id).concat(all.slice(0, 40));
  rows(hit, 'Hech narsa topilmadi.');
}
$('#pInput').addEventListener('input', (e) => search(e.target.value));
$('#pInput').addEventListener('keydown', (e) => e.stopPropagation());
// listening minutes, counted while playing
setInterval(() => { if (S.playing && !audio.paused) store.set('listened', store.get('listened', 0) + 5); }, 5000);
audio.addEventListener('playing', () => { if (audio.dataset.counted !== audio.dataset.id) { audio.dataset.counted = audio.dataset.id; store.set('songs', store.get('songs', 0) + 1); } });


/* ───────────── settings: the gear opens a module with real switches */
function openSettings() {
  screen = 5;
  navKeys.forEach((k) => on(k, false));
  on(gearKey, true);
  const p = $('#panel');
  p.hidden = false; p.classList.remove('closing');
  p.style.animation = 'none'; void p.offsetWidth; p.style.animation = '';
  $('#pSearch').hidden = true;
  $('#pTitle').textContent = 'SOZLAMALAR'; $('#pSub').textContent = 'KR-01 DESK · v' + VERSION;
  const seg = (name, items, val) => `<div class="seg" data-set="${name}">${items.map(([k, l]) => `<i data-v="${k}" class="${k === val ? 'on' : ''}">${l}</i>`).join('')}</div>`;
  $('#pList').innerHTML = `
    <div class="set"><div><b>Mavzu</b><span>Kunduzgi bo'r yoki tungi grafit korpus</span></div>${seg('theme', [['light', 'KUN'], ['dark', 'TUN']], S.theme)}</div>
    <div class="set"><div><b>Qo'shiq matni</b><span>Matnli qo'shiqlar uchun til</span></div>${seg('lang', [['uz', "O'ZB"], ['rom', 'ROM'], ['jp', '日本']], S.lang)}</div>
    <div class="set"><div><b>Stansiya</b><span>Ilova ochilganda qaysi efir</span></div>${seg('station', [['jpop', 'J-POP'], ['ost', 'OST'], ['riot', 'RIOT']], S.station)}</div>
    <div class="set"><div><b>To'liq ekran</b><span>F11 — kino rejimi kabi</span></div>${seg('full', [['on', 'F11']], '')}</div>`;
}
$('#pList').addEventListener('click', (e) => {
  const i = e.target.closest('.seg i');
  if (!i) return;
  click();
  const name = i.parentElement.dataset.set, v = i.dataset.v;
  if (name === 'theme') { S.theme = v; store.set('theme', v); applyTheme(); }
  if (name === 'lang') { S.lang = v; store.set('lang', v); langUi(); renderLyrics(true); }
  if (name === 'station') tuneStation(v);
  if (name === 'full') invoke('window_action', { action: 'fullscreen' });
  if (name !== 'full') openSettings();
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
  stepPaper(dt);
  on(onAir, S.playing && Math.floor(now / 800) % 2 === 0);
  requestAnimationFrame(frame);
}

/* ───────────── start */
(async function start() {
  langUi();
  stationUi();
  requestAnimationFrame(frame);
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
