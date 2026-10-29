/* Klavar Converter — frontend-only driver for the Rust/WASM engine. */
import init, { wasm_convert, wasm_filename } from "./pkg/klavar_rs.js";

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

function convert() {
  if (!currentData) return;
  clearError();
  let pdf;
  try {
    pdf = wasm_convert(currentData, parseFloat($("quarter").value), parseInt($("hands").value, 10), flags(), parseInt($("columns").value, 10));
  } catch (e) {
    const msg = (e && e.message) || String(e);
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
}

for (const id of ["quarter", "hands", "barNums", "countNums", "subdivs", "columns"])
  $(id).addEventListener("change", convert);
$("quarter").addEventListener("input", () =>
  $("quarterOut").textContent = `${$("quarter").value} mm / quarter`
);
$("columns").addEventListener("input", () => {
  $("columnsOut").textContent = $("columns").value;
  convert(); // discrete steps: safe to re-render live
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
