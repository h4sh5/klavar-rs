/* Klavar Converter — frontend-only driver for the Rust/WASM engine. */
import init, { wasm_convert, wasm_filename, wasm_svg, wasm_timeline } from "./pkg/klavar_rs.js";

const $ = (id) => document.getElementById(id);
const drop = $("drop"), fileInput = $("file");
const result = $("result"), preview = $("preview"), download = $("download"), meta = $("meta");
const errBox = $("error");

let currentData = null; // last input bytes (after MXL extraction)
let currentName = "";
let lastURL = null;

await init();

/* ---------- input handling ---------- */

drop.addEventListener("click", (e) => {
  if (!e.target.closest("button")) fileInput.click();
});
drop.addEventListener("keydown", (e) => {
  if (e.key === "Enter" || e.key === " ") fileInput.click();
});
fileInput.addEventListener("change", () => fileInput.files[0] && load(fileInput.files[0]));
for (const ev of ["dragenter", "dragover"]) drop.addEventListener(ev, (e) => { e.preventDefault(); drop.classList.add("drag"); });
for (const ev of ["dragleave", "drop"]) drop.addEventListener(ev, (e) => { e.preventDefault(); drop.classList.remove("drag"); });
drop.addEventListener("drop", (e) => e.dataTransfer.files[0] && load(e.dataTransfer.files[0]));

document.querySelectorAll("[data-sample]").forEach((b) =>
  b.addEventListener("click", async () => {
    const resp = await fetch(b.dataset.sample);
    if (!resp.ok) return showError("Could not fetch sample " + b.dataset.sample);
    loadBytes(await resp.arrayBuffer(), b.dataset.sample.split("/").pop());
  })
);

async function load(file) {
  loadBytes(await file.arrayBuffer(), file.name);
}

async function loadBytes(buf, name) {
  clearError();
  try {
    buf = await maybeUnzip(buf); // .mxl is zipped MusicXML
  } catch (e) {
    return showError(`Could not unpack "${name}": ${e.message}\n(If this is a protected or unusual ZIP, try uncompressed MusicXML.)`);
  }
  currentData = new Uint8Array(buf);
  currentName = name;
  convert();
}

/* Minimal ZIP reader: returns the largest *.xml member (MusicXML inside .mxl). */
async function maybeUnzip(buf) {
  const u8 = new Uint8Array(buf);
  if (!(u8[0] === 0x50 && u8[1] === 0x4b && u8[2] === 0x03 && u8[3] === 0x04)) return buf;
  let best = null, off = 0;
  const dv = new DataView(buf);
  while (off + 30 <= u8.length && dv.getUint32(off, true) === 0x04034b50) {
    const method = dv.getUint16(off + 8, true);
    const csize = dv.getUint32(off + 18, true);
    const nameLen = dv.getUint16(off + 26, true);
    const extraLen = dv.getUint16(off + 28, true);
    const name = new TextDecoder().decode(u8.subarray(off + 30, off + 30 + nameLen));
    const dataStart = off + 30 + nameLen + extraLen;
    if (csize > 0 && /\.xml$/i.test(name)) {
      const raw = u8.subarray(dataStart, dataStart + csize);
      if (!best || csize > best.size) best = { name, raw, method, size: csize };
    }
    off = dataStart + csize;
  }
  if (!best) throw new Error("no .xml entry found");
  if (best.method === 0) return best.raw.slice().buffer;
  if (best.method === 8) {
    const ds = new DecompressionStream("deflate-raw");
    const stream = new Blob([best.raw]).stream().pipeThrough(ds);
    return await new Response(stream).arrayBuffer();
  }
  throw new Error("unsupported compression method " + best.method);
}

/* ---------- conversion ---------- */

function flags() {
  let f = 0;
  if ($("barNums").checked) f |= 1;
  if ($("countNums").checked) f |= 2;
  if ($("subdivs").checked) f |= 4;
  return f;
}

function currentOpts() {
  return [
    parseFloat($("quarter").value),
    parseInt($("hands").value, 10),
    flags(),
    parseInt($("columns").value, 10),
    parseFloat($("noteSize").value),
  ];
}

function convert() {
  if (!currentData) return;
  clearError();
  let pdf, svgStr, tlJson;
  try {
    const o = currentOpts();
    pdf = wasm_convert(currentData, ...o);
    svgStr = wasm_svg(currentData, ...o);
    tlJson = wasm_timeline(currentData);
  } catch (e) {
    const msg = (e && e.message) || String(e);
    playerStopAll();
    $("player").hidden = true;
    $("gotoPlayer").hidden = true;
    return showError(
      msg.includes("Parse error")
        ? `Could not parse "${currentName}".\n${msg}`
        : msg
    );
  }
  if (lastURL) URL.revokeObjectURL(lastURL);
  const blob = new Blob([pdf], { type: "application/pdf" });
  lastURL = URL.createObjectURL(blob);
  preview.data = lastURL;
  download.href = lastURL;
  download.download = wasm_filename(currentData);
  meta.textContent = `${currentName} → ${download.download} · ${(pdf.length / 1024).toFixed(1)} KB · rendered locally`;
  result.hidden = false;

  // interactive view
  $("svgWrap").innerHTML = svgStr;
  playerLoad(JSON.parse(tlJson));
  $("player").hidden = false;
  $("gotoPlayer").hidden = false;
}

for (const id of ["quarter", "hands", "barNums", "countNums", "subdivs", "columns", "noteSize"])
  $(id).addEventListener("change", convert);
$("quarter").addEventListener("input", () =>
  $("quarterOut").textContent = `${$("quarter").value} mm / quarter`
);
$("columns").addEventListener("input", () => {
  $("columnsOut").textContent = $("columns").value;
  convert(); // discrete steps: safe to re-render live
});
$("noteSize").addEventListener("input", () =>
  $("noteSizeOut").textContent = `${Math.round($("noteSize").value * 100)}%`
);

/* ---------- interactive player ----------
 * Timing model: the score position is derived from the AudioContext clock —
 *   tick = anchorTick + (ctx.currentTime - anchorTime) * speed / secPerTick
 * so playback, highlighting and the speed slider can never drift apart. */

const playerSec = $("player"), svgWrap = $("svgWrap"), playBtn = $("playBtn"),
  restartBtn = $("restartBtn"), ptime = $("ptime"), speedIn = $("speed"),
  speedOut = $("speedOut"), progressFill = $("progressFill"),
  zoomIn = $("zoom"), zoomOut = $("zoomOut");

/* display zoom for the score; applied as a CSS var so the SVG pages scale up
 * and scroll inside the fixed-height box */
function applyZoom() {
  const z = parseFloat(zoomIn.value) || 2;
  svgWrap.style.setProperty("--pz", z);
  zoomOut.textContent = `${+z.toFixed(2)}\u00d7`;
  // scale both dimensions explicitly: the svg carries width/height attributes,
  // so a CSS height alone would letterbox instead of zoom
  const svg = svgWrap.querySelector("svg");
  if (svg) {
    const vb = (svg.getAttribute("viewBox") || "0 0 595.28 841.89").split(/[\s,]+/);
    svg.style.width = (parseFloat(vb[2]) * z).toFixed(1) + "px";
    svg.style.height = (parseFloat(vb[3]) * z).toFixed(1) + "px";
  }
}
zoomIn.addEventListener("input", applyZoom);
applyZoom();

const P = {
  notes: [],          // {t, e, p} sorted by start tick
  secPerTick: 60 / 120 / 480,
  endTick: 480 * 4,
  speed: 1,
  playing: false,
  anchorTick: 0,      // score position at anchorTime (also the pause position)
  anchorTime: 0,      // AudioContext time of the anchor
  ctx: null, master: null,
  schedIdx: 0,        // audio look-ahead scheduler pointer
  live: [],           // {gain, oscs, stopAt} scheduled/pending nodes
  active: [],         // currently lit note descriptors
  scan: 0,            // highlight pointer into P.notes
  timer: 0, raf: 0,
};
const elemCache = new Map();

function playerLoad(tl) {
  playerStopAll();
  P.notes = tl.notes.map(([t, e, p]) => ({ t, e, p })).sort((a, b) => a.t - b.t || a.p - b.p);
  P.secPerTick = 60 / tl.bpm / tl.ppq;
  P.endTick = Math.max(tl.endTick, 1);
  P.anchorTick = 0;
  P.scan = 0;
  P.speed = parseFloat(speedIn.value) || 0.5; // keep in sync with the slider
  speedOut.textContent = `${P.speed.toFixed(2)}\u00d7`;
  elemCache.clear();
  applyZoom(); // re-apply after the SVG was replaced
  setPlayBtn();
  updatePlayerUI(0);
  playBtn.disabled = P.notes.length === 0;
}

function ensureCtx() {
  if (!P.ctx) {
    const AC = window.AudioContext || window.webkitAudioContext;
    P.ctx = new AC();
    const comp = P.ctx.createDynamicsCompressor();
    P.master = P.ctx.createGain();
    P.master.gain.value = 0.9;
    P.master.connect(comp);
    comp.connect(P.ctx.destination);
  }
}

const tickNow = () =>
  P.playing
    ? P.anchorTick + ((P.ctx.currentTime - P.anchorTime) * P.speed) / P.secPerTick
    : P.anchorTick;

/* first note that still sounds at or after `tick` */
function lowerBound(tick) {
  let lo = 0, hi = P.notes.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (P.notes[mid].e <= tick) lo = mid + 1; else hi = mid;
  }
  return lo;
}

async function playerPlay() {
  if (!P.notes.length || P.playing) return;
  ensureCtx();
  await P.ctx.resume();
  if (P.anchorTick >= P.endTick) playerSeek(0); // finished -> restart
  P.anchorTime = P.ctx.currentTime + 0.06;
  P.schedIdx = lowerBound(P.anchorTick);
  P.playing = true;
  rebuildHighlight();
  setPlayBtn();
  scheduleAhead();
  P.timer = setInterval(scheduleAhead, 25);
  cancelAnimationFrame(P.raf);
  P.raf = requestAnimationFrame(frame);
}

function playerPause() {
  if (!P.playing) return;
  P.anchorTick = tickNow();
  P.playing = false;
  clearInterval(P.timer); P.timer = 0;
  cancelAnimationFrame(P.raf);
  stopAllAudio();
  setPlayBtn();
  updatePlayerUI(P.anchorTick);
}

/* full stop: silence + back to the start (keeps notes lit-off) */
function playerStopAll() {
  playerPause();
  playerSeek(0);
}

function playerSeek(tick) {
  const wasPlaying = P.playing;
  if (wasPlaying) { clearInterval(P.timer); cancelAnimationFrame(P.raf); }
  stopAllAudio();
  P.anchorTick = Math.max(0, Math.min(tick, P.endTick));
  P.schedIdx = lowerBound(P.anchorTick);
  rebuildHighlight();
  updatePlayerUI(P.anchorTick);
  if (wasPlaying) {
    P.anchorTime = P.ctx.currentTime + 0.03;
    scheduleAhead();
    P.timer = setInterval(scheduleAhead, 25);
    P.raf = requestAnimationFrame(frame);
  }
}

/* ---------- audio ---------- */

function scheduleAhead() {
  const ctx = P.ctx, LOOK = 0.22;
  while (P.schedIdx < P.notes.length) {
    const n = P.notes[P.schedIdx];
    const when = P.anchorTime + ((n.t - P.anchorTick) * P.secPerTick) / P.speed;
    if (when > ctx.currentTime + LOOK) break;
    P.schedIdx++;
    if (n.e <= P.anchorTick) continue; // already behind the play position
    const start = Math.max(when, ctx.currentTime + 0.004);
    const from = Math.max(n.t, P.anchorTick);
    const stop = start + Math.max(((n.e - from) * P.secPerTick) / P.speed, 0.07);
    scheduleNote(n.p, start, stop);
  }
  // housekeeping: drop finished nodes
  const now = ctx.currentTime;
  P.live = P.live.filter((v) => v.stopAt > now + 0.2);
}

function scheduleNote(pitch, start, stop) {
  const ctx = P.ctx;
  const freq = 440 * Math.pow(2, (pitch - 69) / 12);
  const gain = ctx.createGain();
  const peak = 0.22;
  const rel = Math.max(stop - 0.01, start + 0.05);
  gain.gain.setValueAtTime(0, start);
  gain.gain.linearRampToValueAtTime(peak, start + 0.012);
  gain.gain.exponentialRampToValueAtTime(peak * 0.55, Math.min(start + 0.3, rel));
  gain.gain.setTargetAtTime(0.0001, rel, 0.035);
  gain.connect(P.master);

  const oscs = [];
  const add = (type, mult, vol, det) => {
    const o = ctx.createOscillator();
    const g = ctx.createGain();
    o.type = type;
    o.frequency.value = freq * mult;
    if (det) o.detune.value = det;
    g.gain.value = vol;
    o.connect(g); g.connect(gain);
    o.start(start); o.stop(rel + 0.25);
    oscs.push(o);
  };
  add("triangle", 1, 1, (Math.random() - 0.5) * 6);
  add("sine", 2, 0.28, 0);
  add("sine", 1, 0.5, (Math.random() - 0.5) * 4);
  P.live.push({ gain, oscs, stopAt: rel + 0.3 });
}

function stopAllAudio() {
  const ctx = P.ctx;
  if (!ctx) { P.live = []; return; }
  const now = ctx.currentTime;
  for (const v of P.live) {
    try {
      v.gain.gain.cancelScheduledValues(now);
      v.gain.gain.setTargetAtTime(0.0001, now, 0.012);
      for (const o of v.oscs) o.stop(now + 0.06);
    } catch (_) { /* already stopped */ }
  }
  P.live = [];
}

/* ---------- note highlighting ---------- */

function noteEls(n) {
  const key = n.t + ":" + n.p;
  let els = elemCache.get(key);
  if (els === undefined) {
    els = Array.from(svgWrap.querySelectorAll(`[data-n="${key}"]`));
    elemCache.set(key, els);
  }
  return els;
}

function litOn(n) {
  const els = noteEls(n);
  for (const el of els) el.classList.add("on");
  P.active.push({ n, els });
  return els.length > 0;
}

function litOff(a) {
  for (const el of a.els) el.classList.remove("on");
}

function rebuildHighlight() {
  for (const a of P.active) litOff(a);
  P.active = [];
  P.scan = 0;
  const cur = P.anchorTick;
  while (P.scan < P.notes.length && P.notes[P.scan].t <= cur) {
    if (P.notes[P.scan].e > cur) litOn(P.notes[P.scan]);
    P.scan++;
  }
}

function frame() {
  if (!P.playing) return;
  const cur = tickNow();

  // light up newly started notes
  let fresh = null;
  while (P.scan < P.notes.length && P.notes[P.scan].t <= cur) {
    const n = P.notes[P.scan];
    if (n.e > cur && litOn(n) && !fresh) fresh = n;
    P.scan++;
  }
  // release finished notes
  P.active = P.active.filter((a) => {
    if (a.n.e <= cur) { litOff(a); return false; }
    return true;
  });

  updatePlayerUI(cur);
  if (fresh) followNote(fresh);

  if (cur >= P.endTick) {
    P.anchorTick = P.endTick;
    playerPause();
    updatePlayerUI(P.endTick);
    return;
  }
  P.raf = requestAnimationFrame(frame);
}

function followNote(n) {
  const els = noteEls(n);
  if (!els.length) return;
  const el = els[0];
  const wr = svgWrap.getBoundingClientRect();
  const er = el.getBoundingClientRect();
  if (er.top < wr.top + 30 || er.bottom > wr.bottom - 30) {
    svgWrap.scrollTop += er.top - wr.top - wr.height * 0.35;
  }
  if (er.left < wr.left || er.right > wr.right) {
    svgWrap.scrollLeft += er.left - wr.left - wr.width * 0.3;
  }
}

/* ---------- player UI ---------- */

function fmt(sec) {
  sec = Math.max(0, sec);
  const m = Math.floor(sec / 60), s = Math.floor(sec % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function updatePlayerUI(tick) {
  const sp = P.secPerTick;
  ptime.textContent = `${fmt(tick * sp)} / ${fmt(P.endTick * sp)}`;
  progressFill.style.width = `${Math.min(100, (tick / P.endTick) * 100).toFixed(2)}%`;
}

function setPlayBtn() {
  playBtn.innerHTML = P.playing ? "&#10074;&#10074; Pause" : "&#9654; Play";
}

function togglePlay() {
  P.playing ? playerPause() : playerPlay();
}

playBtn.addEventListener("click", togglePlay);
restartBtn.addEventListener("click", () => playerSeek(0));
$("gotoPlayer").addEventListener("click", () => {
  $("player").scrollIntoView({ behavior: "smooth", block: "start" });
  $("player").focus({ preventScroll: true });
});

speedIn.addEventListener("input", () => {
  const wasPlaying = P.playing;
  P.speed = parseFloat(speedIn.value) || 0.5;
  speedOut.textContent = `${P.speed.toFixed(2)}\u00d7`;
  if (wasPlaying) {
    const cur = tickNow();
    stopAllAudio();
    P.anchorTick = cur;
    P.anchorTime = P.ctx.currentTime + 0.02;
    P.schedIdx = lowerBound(cur);
    scheduleAhead();
  }
});

document.addEventListener("keydown", (e) => {
  if (e.code !== "Space" || $("player").hidden) return;
  const t = e.target;
  if (t && (t.tagName === "INPUT" || t.tagName === "BUTTON" || t.tagName === "SELECT" || t.isContentEditable)) return;
  e.preventDefault();
  togglePlay();
});

/* ---------- errors ---------- */

function showError(msg) {
  errBox.textContent = msg;
  errBox.hidden = false;
  result.hidden = true;
}
function clearError() {
  errBox.hidden = true;
}
