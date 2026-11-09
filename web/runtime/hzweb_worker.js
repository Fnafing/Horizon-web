// A pooled std::thread worker: load the glue, go idle, wait for a job, run it, report done.
import { hzwebSetContext } from './hzweb.js';
const FIELDS = 4, IDLE = 1, JOB = 3, RUNNING = 4, DONE = 5;
self.onmessage = async (e) => {
  const { module, memory, glue, slots, count, index } = e.data;
  const m = await import(glue);
  hzwebSetContext({ module, memory, glue, slots, count });
  const at = index * FIELDS;
  Atomics.store(slots, at, IDLE);
  while (Atomics.load(slots, at) !== JOB) Atomics.wait(slots, at, Atomics.load(slots, at));
  const arg = slots[at + 1] >>> 0, stack = slots[at + 2];
  Atomics.store(slots, at, RUNNING);
  const exports = m.initSync({ module, memory, thread_stack_size: stack });
  try {
    m.hzweb_thread_start(arg);
  } catch (err) {
    console.error('hzweb thread panicked:', err);
  }
  if (exports.__wbindgen_thread_destroy) exports.__wbindgen_thread_destroy();
  Atomics.store(slots, at, DONE);
  // Wake spawners waiting for a free slot.
  Atomics.add(slots, count * FIELDS, 1);
  Atomics.notify(slots, count * FIELDS);
  postMessage('done');
};
