// Runs the Rust engine off the page's thread. SQLite's browser storage (OPFS) only works inside a worker.
import init, * as engine from './pkg/foryooru_web.js';
import wasmUrl from './pkg/foryooru_web_bg.wasm?url';

export type Request =
  | { type: 'start'; site: string; settings: Record<string, unknown> }
  | { type: 'settings'; settings: Record<string, unknown> }
  | { type: 'invoke'; id: number; command: string; args?: Record<string, unknown> }
  | { type: 'export'; id: number; settings: Record<string, unknown> }
  | { type: 'import'; id: number; bytes: Uint8Array };

export type Response =
  | { type: 'ready' }
  | { type: 'failed'; error: unknown }
  | { type: 'reply'; id: number; ok: true; value: unknown }
  | { type: 'reply'; id: number; ok: false; error: unknown }
  | { type: 'event'; event: string; payload: unknown };

const send = (message: Response, transfer: Transferable[] = []) => postMessage(message, { transfer });

// commands can arrive while the wasm is still loading, they wait for it
const started = new Promise<void>((resolve) => {
  addEventListener('message', async function onStart(message: MessageEvent<Request>) {
    if (message.data.type !== 'start') return;
    removeEventListener('message', onStart);
    try {
      await init({ module_or_path: wasmUrl });
      const emit = (event: string, payload: unknown) => send({ type: 'event', event, payload });
      await engine.start(message.data.site, message.data.settings, emit);
      send({ type: 'ready' });
      resolve();
    } catch (error) {
      send({ type: 'failed', error });
    }
  });
});

addEventListener('message', async (message: MessageEvent<Request>) => {
  const request = message.data;
  if (request.type === 'start') return;
  await started;
  if (request.type === 'settings') {
    engine.set_settings(request.settings);
    return;
  }
  try {
    if (request.type === 'invoke') {
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
