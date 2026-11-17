// The page's side of the engine worker: Tauri-style invoke() and listen() over postMessage.
import EngineWorker from './worker?worker';
import type { Request, Response } from './worker';

const worker = new EngineWorker();
const waiting = new Map<number, { resolve: (value: unknown) => void; reject: (error: unknown) => void }>();
const listeners = new Map<string, Set<(payload: unknown) => void>>();
let nextId = 0;

let markReady: () => void;
let markFailed: (error: unknown) => void;
export const ready = new Promise<void>((resolve, reject) => {
  markReady = resolve;
  markFailed = reject;
});

worker.addEventListener('message', (message: MessageEvent<Response>) => {
  const response = message.data;
  if (response.type === 'ready') markReady();
  else if (response.type === 'failed') markFailed(response.error);
  else if (response.type === 'event') listeners.get(response.event)?.forEach((listener) => listener(response.payload));
  else {
    const pending = waiting.get(response.id);
    waiting.delete(response.id);
    if (response.ok) pending?.resolve(response.value);
    else pending?.reject(response.error);
  }
});

const post = (request: Request, transfer: Transferable[] = []) => worker.postMessage(request, { transfer });

function call<T>(build: (id: number) => Request, transfer: Transferable[] = []): Promise<T> {
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    waiting.set(id, { resolve: resolve as (value: unknown) => void, reject });
    post(build(id), transfer);
  });
}

export function startEngine(site: string, settings: Record<string, unknown>) {
  post({ type: 'start', site, settings });
  return ready;
}

export const sendSettings = (settings: Record<string, unknown>) => post({ type: 'settings', settings });

export const invoke = <T>(command: string, args?: Record<string, unknown>) =>
  call<T>((id) => ({ type: 'invoke', id, command, args }));

export const exportBackup = (settings: Record<string, unknown>) =>
  call<Uint8Array>((id) => ({ type: 'export', id, settings }));

export const importBackup = (bytes: Uint8Array) =>
  call<Record<string, unknown> | null>((id) => ({ type: 'import', id, bytes }), [bytes.buffer]);

export function listen<T>(event: string, listener: (payload: T) => void) {
  const set = listeners.get(event) ?? new Set();
  set.add(listener as (payload: unknown) => void);
  listeners.set(event, set);
  return () => set.delete(listener as (payload: unknown) => void);
}
