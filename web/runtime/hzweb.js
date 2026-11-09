// Horizon web runtime: the "hzweb" imports of the patched std (clock, cores, threads).
// Threads come from a pool of pre-started module Workers that wait on a shared slot: a
// spawn claims an idle slot and wakes it with Atomics.notify, so spawning works even while
// the spawning thread is blocked (a blocked thread can't start a Worker itself). The main
// thread refills the pool as workers finish.
const IS_MAIN = typeof document !== 'undefined';
const FIELDS = 4; // state, arg, stack, spare
// slot states
const STARTING = 0, IDLE = 1, CLAIMED = 2, JOB = 3, RUNNING = 4, DONE = 5;
let ctx = null; // { module, memory, glue, slots: Int32Array, count }

export function hzwebSetContext(c) { ctx = c; }
export function hzwebContext() { return ctx; }
export function hz_now_ms() { return performance.timeOrigin + performance.now(); }
export function hz_date_ms() { return Date.now(); }
export function hz_cores() { return Math.max(2, Math.min(navigator.hardwareConcurrency || 4, 16)); }
export function hz_is_main() { return IS_MAIN ? 1 : 0; }
const lines = { 1: '', 2: '' };
const decoder = new TextDecoder();
export function hz_write(fd, ptr, len) {
  if (!ctx || !ctx.memory) return;
  // TextDecoder refuses views of shared memory: copy first.
  lines[fd] = (lines[fd] || '') + decoder.decode(new Uint8Array(ctx.memory.buffer, ptr >>> 0, len >>> 0).slice());
  let nl;
  while ((nl = lines[fd].indexOf('\n')) >= 0) {
    const line = lines[fd].slice(0, nl);
    lines[fd] = lines[fd].slice(nl + 1);
    (fd === 2 ? console.error : console.log)(line);
  }
}

export function hz_spawn(arg, stack) {
  if (!ctx || !ctx.slots) { console.error('hzweb: spawn before the thread pool started'); return 1; }
  const s = ctx.slots, n = ctx.count;
  const pages = Math.ceil(Math.max(stack, 1 << 20) / 65536) * 65536;
  for (let round = 0; round < 2000; round++) {
    for (let i = 0; i < n; i++) {
      if (Atomics.compareExchange(s, i * FIELDS, IDLE, CLAIMED) === IDLE) {
        s[i * FIELDS + 1] = arg;
        s[i * FIELDS + 2] = pages;
        Atomics.store(s, i * FIELDS, JOB);
        Atomics.notify(s, i * FIELDS);
        return 0;
      }
    }
    // Every worker is busy: wait a moment for one to come back (not possible on main).
    if (IS_MAIN) break;
    Atomics.wait(s, n * FIELDS, 0, 5);
  }
  console.error(`hzweb: no idle thread among ${n}`);
  return 1;
}

/** Main thread: start `count` pooled workers on `memory`, and refill slots as they finish. */
export function hzwebStartPool({ module, memory, glue, count = 64 }) {
  const slots = new Int32Array(new SharedArrayBuffer((count * FIELDS + 1) * 4));
  ctx = { module, memory, glue, slots, count };
  const start = (i) => {
    Atomics.store(slots, i * FIELDS, STARTING);
    const w = new Worker(new URL('./hzweb_worker.js', import.meta.url), { type: 'module', name: `hz-thread-${i}` });
    w.onmessage = (e) => {
      if (e.data === 'done') { w.terminate(); start(i); }
    };
    w.onerror = (e) => { console.error('hzweb thread worker', i, e.message || e); };
    w.postMessage({ module, memory, glue, slots, count, index: i });
  };
  for (let i = 0; i < count; i++) start(i);
  return ctx;
}

/** Resolves once every pooled worker is idle (ready to take a spawn). */
export async function hzwebPoolReady() {
  const { slots, count } = ctx;
  for (;;) {
    let ready = 0;
    for (let i = 0; i < count; i++) if (Atomics.load(slots, i * FIELDS) !== STARTING) ready++;
    if (ready === count) return;
    await new Promise((r) => setTimeout(r, 20));
  }
}
