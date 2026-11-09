// Horizon Web launcher: keeps the player's .hzpack and saves in IndexedDB, fills the game's
// virtual filesystem from them, starts the thread pool and runs the game.
import init, * as hz from './horizon_web.js';
import { hzwebStartPool, hzwebPoolReady } from './hzweb.js';

const RESOURCES = '/horizon/resources';
const HOME = '/home/player/horizon';
const PERSIST_PREFIX = '/home/';
const $ = (id) => document.getElementById(id);
const status = (msg, bad = false) => { $('status').textContent = msg; $('status').className = bad ? 'bad' : ''; };
const bar = (v) => { const b = $('bar'); if (v == null) { b.className = ''; } else { b.className = 'on'; b.value = v; } };

// ---------- IndexedDB ----------
function db() {
  return new Promise((ok, fail) => {
    const r = indexedDB.open('horizon-web', 1);
    r.onupgradeneeded = () => { r.result.createObjectStore('pack'); r.result.createObjectStore('files'); };
    r.onsuccess = () => ok(r.result);
    r.onerror = () => fail(r.error);
  });
}
async function tx(store, mode, fn) {
  const d = await db();
  return new Promise((ok, fail) => {
    const t = d.transaction(store, mode);
    const s = t.objectStore(store);
    let out;
    Promise.resolve(fn(s)).then((v) => { out = v; });
    t.oncomplete = () => ok(out);
    t.onerror = () => fail(t.error);
  });
}
const req = (r) => new Promise((ok, fail) => { r.onsuccess = () => ok(r.result); r.onerror = () => fail(r.error); });
const getPack = () => tx('pack', 'readonly', (s) => req(s.get('current')));
const putPack = (v) => tx('pack', 'readwrite', (s) => { s.put(v, 'current'); });
const delPack = () => tx('pack', 'readwrite', (s) => { s.delete('current'); });
async function allFiles() {
  return tx('files', 'readonly', async (s) => {
    const [keys, vals] = await Promise.all([req(s.getAllKeys()), req(s.getAll())]);
    return keys.map((k, i) => [k, vals[i]]);
  });
}

// ---------- zip ----------
async function inflateRaw(bytes) {
  const ds = new DecompressionStream('deflate-raw');
  const out = new Response(new Blob([bytes]).stream().pipeThrough(ds));
  return new Uint8Array(await out.arrayBuffer());
}
function zipEntries(buf) {
  const v = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 65557); i--) {
    if (v.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
  }
  if (eocd < 0) throw new Error('not a zip / .hzpack file');
  let count = v.getUint16(eocd + 10, true);
  let cd = v.getUint32(eocd + 16, true);
  if (cd === 0xffffffff || count === 0xffff) {
    const loc = v.getUint32(eocd - 20 + 8, true); // zip64 EOCD locator -> record
    count = Number(v.getBigUint64(loc + 32, true));
    cd = Number(v.getBigUint64(loc + 48, true));
  }
  const dec = new TextDecoder();
  const out = [];
  let p = cd;
  for (let n = 0; n < count; n++) {
    if (v.getUint32(p, true) !== 0x02014b50) throw new Error('broken zip directory');
    const method = v.getUint16(p + 10, true);
    let csize = v.getUint32(p + 20, true), size = v.getUint32(p + 24, true);
    const nlen = v.getUint16(p + 28, true), xlen = v.getUint16(p + 30, true), clen = v.getUint16(p + 32, true);
    let off = v.getUint32(p + 42, true);
    const name = dec.decode(buf.subarray(p + 46, p + 46 + nlen));
    let x = p + 46 + nlen; const xend = x + xlen;
    while (x + 4 <= xend) {
      const id = v.getUint16(x, true), len = v.getUint16(x + 2, true);
      if (id === 1) { // zip64
        let q = x + 4;
        if (size === 0xffffffff) { size = Number(v.getBigUint64(q, true)); q += 8; }
        if (csize === 0xffffffff) { csize = Number(v.getBigUint64(q, true)); q += 8; }
        if (off === 0xffffffff) { off = Number(v.getBigUint64(q, true)); q += 8; }
      }
      x += 4 + len;
    }
    out.push({ name, method, csize, size, off });
    p = xend + clen;
  }
  return out.map((e) => {
    const ln = v.getUint16(e.off + 26, true), lx = v.getUint16(e.off + 28, true);
    return { ...e, start: e.off + 30 + ln + lx };
  });
}

// ---------- UI ----------
let packMeta = null;
async function refresh() {
  const p = await getPack().catch(() => null);
  packMeta = p ? p.meta : null;
  $('play').disabled = !p;
  $('forget').disabled = !p;
  $('pack').textContent = p
    ? `Pack: ${p.name} — Minecraft ${p.meta?.minecraft ?? '?'}, ${(p.blob.size / 1e6).toFixed(0)} MB, sound mode ${p.meta?.sound ?? 1}`
    : 'No pack yet.';
}
async function takeFile(f) {
  if (!f) return;
  status('Checking ' + f.name + ' …');
  try {
    const head = new Uint8Array(await f.slice(Math.max(0, f.size - 65557)).arrayBuffer());
    if (!head.length) throw new Error('empty file');
    const all = new Uint8Array(await f.arrayBuffer());
    const entries = zipEntries(all);
    const m = entries.find((e) => e.name === 'hzpack.json');
    if (!m) throw new Error('no hzpack.json inside — make it with mcpack');
    let raw = all.subarray(m.start, m.start + m.csize);
    if (m.method === 8) raw = await inflateRaw(raw);
    const meta = JSON.parse(new TextDecoder().decode(raw));
    await putPack({ name: f.name, blob: f, meta });
    status('Pack stored.');
  } catch (e) {
    status('That file did not work: ' + e.message, true);
  }
  refresh();
}
const drop = $('drop');
drop.onclick = () => $('file').click();
drop.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ' ') $('file').click(); };
$('file').onchange = (e) => takeFile(e.target.files[0]);
drop.ondragover = (e) => { e.preventDefault(); drop.classList.add('over'); };
drop.ondragleave = () => drop.classList.remove('over');
drop.ondrop = (e) => { e.preventDefault(); drop.classList.remove('over'); takeFile(e.dataTransfer.files[0]); };
$('forget').onclick = async () => { await delPack(); status('Pack removed from this browser.'); refresh(); };
$('wipe').onclick = async () => {
  if (!confirm('Delete all worlds, options and logs stored in this browser?')) return;
  await tx('files', 'readwrite', (s) => { s.clear(); });
  status('Saves deleted.');
};

// ---------- game ----------
function persistLoop() {
  let busy = false;
  const drain = async () => {
    if (busy) return;
    busy = true;
    try {
      const puts = [], dels = [];
      for (let i = 0; i < 5000; i++) {
        const c = hz.hz_take_change();
        if (c === undefined) break;
        const [path, data] = c;
        if (!path.startsWith(PERSIST_PREFIX)) continue;
        if (data === null) dels.push(path.replace(/\/$/, ''), path.replace(/\/$/, '') + '/');
        else puts.push([path, data]);
      }
      if (puts.length || dels.length) {
        await tx('files', 'readwrite', (s) => {
          for (const k of dels) s.delete(k);
          for (const [k, v] of puts) s.put(v, k);
        });
      }
    } catch (e) {
      console.warn('saving to IndexedDB failed', e);
    } finally {
      busy = false;
    }
  };
  setInterval(drain, 3000);
  addEventListener('visibilitychange', () => { if (document.hidden) drain(); });
  addEventListener('pagehide', drain);
}

$('play').onclick = async () => {
  if (!crossOriginIsolated) {
    status('This page needs to be opened through mcpack\'s "Launch Web" (it sets the headers browsers require for threads).', true);
    return;
  }
  $('play').disabled = true;
  try {
    status('Loading the game …'); bar(0.02);
    const module = await WebAssembly.compileStreaming(fetch('./horizon_web_bg.wasm'));
    const exports = await init({ module_or_path: module });
    const memory = exports.memory;
    const glue = new URL('./horizon_web.js', location.href).href;

    status('Unpacking your .hzpack …'); bar(0.1);
    const p = await getPack();
    const all = new Uint8Array(await p.blob.arrayBuffer());
    const entries = zipEntries(all).filter((e) => e.name.startsWith('res/') && !e.name.endsWith('/'));
    hz.hz_mkdirs(RESOURCES);
    for (let i = 0; i < entries.length; i++) {
      const e = entries[i];
      let bytes = all.subarray(e.start, e.start + e.csize);
      if (e.method === 8) bytes = await inflateRaw(bytes);
      else if (e.method !== 0) throw new Error(`unsupported compression in ${e.name}`);
      hz.hz_put(RESOURCES + '/' + e.name.slice(4), bytes, false);
      if (i % 500 === 0) { bar(0.1 + 0.7 * (i / entries.length)); status(`Unpacking your .hzpack … ${i}/${entries.length}`); await new Promise((r) => setTimeout(r)); }
    }

    status('Restoring saves …'); bar(0.85);
    hz.hz_mkdirs(HOME);
    for (const [path, data] of await allFiles()) {
      if (path.endsWith('/')) hz.hz_mkdirs(path.slice(0, -1));
      else hz.hz_put(path, data, false);
    }

    status('Starting threads …'); bar(0.92);
    hzwebStartPool({ module, memory, glue, count: 64 });
    await hzwebPoolReady();

    bar(1); status('Starting Minecraft …');
    persistLoop();
    $('ui').classList.add('hidden');
    try {
      hz.hz_run(['--resources', RESOURCES, '--home', HOME, '--game-dir', HOME]);
    } catch (e) {
      // winit hands control to the browser by throwing this.
      if (!String(e).includes('Using exceptions for control flow')) throw e;
    }
  } catch (e) {
    console.error(e);
    $('ui').classList.remove('hidden');
    status('The game stopped: ' + (e?.message || e), true);
    $('play').disabled = false;
  }
};

if (!crossOriginIsolated) {
  status('Open this page through mcpack\'s "Launch Web" — browsers only allow the game\'s threads with the headers it sets.', true);
}
refresh();
