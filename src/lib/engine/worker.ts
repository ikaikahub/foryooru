// Runs the Rust engine off the page's thread. SQLite's browser storage (OPFS) only works inside a worker.
import init, * as engine from './pkg/foryooru_web.js';
import wasmUrl from './pkg/foryooru_web_bg.wasm?url';

// what a tab asks of the engine; ping answers once the engine has started
export type Call =
  | { type: 'ping' }
  | { type: 'invoke'; command: string; args?: Record<string, unknown> }
  | { type: 'export'; settings: Record<string, unknown> }
  | { type: 'import'; bytes: Uint8Array };

export type Request =
  | { type: 'start'; site: string; settings: Record<string, unknown> }
  | { type: 'settings'; settings: Record<string, unknown> }
  | (Call & { id: number });

export type Response =
  | { type: 'reply'; id: number; ok: true; value: unknown }
  | { type: 'reply'; id: number; ok: false; error: unknown }
  | { type: 'event'; event: string; payload: unknown };

const send = (message: Response, transfer: Transferable[] = []) => postMessage(message, { transfer });

// only one tab at a time can hold the database files; when tabs hand over, the old one may still be letting go
const LOCKED_RETRIES = 10;
const LOCKED_RETRY_MS = 300;
const isLocked = (error: unknown) => String((error as { message?: string })?.message).includes('NoModificationAllowedError');

async function startEngine(site: string, settings: Record<string, unknown>, emit: (event: string, payload: unknown) => void) {
  for (let attempt = 1; ; attempt++) {
    try {
      return await engine.start(site, settings, emit);
    } catch (error) {
      if (!isLocked(error) || attempt === LOCKED_RETRIES) throw error;
      await new Promise((resolve) => setTimeout(resolve, LOCKED_RETRY_MS));
    }
  }
}

// calls can arrive while the wasm is still loading, they wait for it; if starting fails, they all get the error
const started = new Promise<void>((resolve, reject) => {
  addEventListener('message', async function onStart(message: MessageEvent<Request>) {
    if (message.data.type !== 'start') return;
    removeEventListener('message', onStart);
    try {
      await init({ module_or_path: wasmUrl });
      const emit = (event: string, payload: unknown) => send({ type: 'event', event, payload });
      await startEngine(message.data.site, message.data.settings, emit);
      resolve();
    } catch (error) {
      reject(error);
    }
  });
});
started.catch(() => {});

addEventListener('message', async (message: MessageEvent<Request>) => {
  const request = message.data;
  if (request.type === 'start') return;
  if (request.type === 'settings') {
    started.then(() => engine.set_settings(request.settings), () => {});
    return;
  }
  try {
    await started;
    if (request.type === 'ping') {
      send({ type: 'reply', id: request.id, ok: true, value: null });
    } else if (request.type === 'invoke') {
      send({ type: 'reply', id: request.id, ok: true, value: await engine.invoke(request.command, request.args) });
    } else if (request.type === 'export') {
      const bytes = engine.export_backup(request.settings);
      send({ type: 'reply', id: request.id, ok: true, value: bytes }, [bytes.buffer]);
    } else {
      send({ type: 'reply', id: request.id, ok: true, value: engine.import_backup(request.bytes) });
    }
  } catch (error) {
    send({ type: 'reply', id: request.id, ok: false, error });
  }
});
