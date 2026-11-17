// The page's side of the engine: Tauri-style invoke() and listen().
// Only one tab can hold the database files at a time, so the tab holding the 'foryooru-engine' lock runs the
// engine worker and the other tabs send their calls to it over a BroadcastChannel. When that tab closes, the
// browser hands the lock to another tab, which starts the engine and asks the others to resend what was unanswered.
import EngineWorker from './worker?worker';
import type { Call, Request, Response } from './worker';

type TabMessage =
  | { type: 'call'; tab: string; id: number; call: Call }
  | { type: 'settings'; settings: Record<string, unknown> }
  | { type: 'reply'; tab: string; id: number; ok: boolean; value?: unknown; error?: unknown }
  | { type: 'event'; event: string; payload: unknown }
  | { type: 'leader' };

type Pending = { call: Call; overChannel: boolean; resolve: (value: unknown) => void; reject: (error: unknown) => void };

const LOCK = 'foryooru-engine';
const TAB = crypto.randomUUID();
const channel = new BroadcastChannel(LOCK);
const waiting = new Map<number, Pending>();
const listeners = new Map<string, Set<(payload: unknown) => void>>();
let nextId = 0;

// set while this tab runs the engine
let worker: Worker | null = null;
// calls from other tabs that the worker is answering, by the worker's id
const forwarded = new Map<number, { tab: string; id: number }>();
// a resent call may already have reached this tab, running it twice would undo toggles like a like
const received = new Set<string>();

function dispatch(event: string, payload: unknown) {
  listeners.get(event)?.forEach((listener) => listener(payload));
}

function settle(id: number, ok: boolean, value: unknown, error: unknown) {
  const pending = waiting.get(id);
  waiting.delete(id);
  if (ok) pending?.resolve(value);
  else pending?.reject(error);
}

function send(id: number, pending: Pending) {
  pending.overChannel = !worker;
  if (worker) worker.postMessage({ ...pending.call, id } satisfies Request);
  else channel.postMessage({ type: 'call', tab: TAB, id, call: pending.call } satisfies TabMessage);
}

function resendUnanswered() {
  for (const [id, pending] of waiting) if (pending.overChannel) send(id, pending);
}

function call<T>(call: Call): Promise<T> {
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    const pending: Pending = { call, overChannel: false, resolve: resolve as (value: unknown) => void, reject };
    waiting.set(id, pending);
    send(id, pending);
  });
}

function startWorker(site: string, settings: Record<string, unknown>) {
  worker = new EngineWorker();
  worker.addEventListener('message', (message: MessageEvent<Response>) => {
    const response = message.data;
    if (response.type === 'event') {
      dispatch(response.event, response.payload);
      channel.postMessage({ type: 'event', event: response.event, payload: response.payload } satisfies TabMessage);
      return;
    }
    const caller = forwarded.get(response.id);
    forwarded.delete(response.id);
    const [value, error] = response.ok ? [response.value, undefined] : [undefined, response.error];
    if (caller) channel.postMessage({ type: 'reply', ...caller, ok: response.ok, value, error } satisfies TabMessage);
    else settle(response.id, response.ok, value, error);
  });
  worker.postMessage({ type: 'start', site, settings } satisfies Request);
}

channel.addEventListener('message', (message: MessageEvent<TabMessage>) => {
  const data = message.data;
  if (data.type === 'call' && worker) {
    const key = `${data.tab}:${data.id}`;
    if (received.has(key)) return;
    received.add(key);
    const id = nextId++;
    forwarded.set(id, { tab: data.tab, id: data.id });
    worker.postMessage({ ...data.call, id } satisfies Request);
  } else if (data.type === 'settings' && worker) {
    worker.postMessage(data satisfies Request);
  } else if (data.type === 'reply' && data.tab === TAB) {
    settle(data.id, data.ok, data.value, data.error);
  } else if (data.type === 'event') {
    dispatch(data.event, data.payload);
  } else if (data.type === 'leader' && !worker) {
    // the tab that was answering may have closed before it did
    resendUnanswered();
  }
});

let markReady: () => void;
let markFailed: (error: unknown) => void;
export const ready = new Promise<void>((resolve, reject) => {
  markReady = resolve;
  markFailed = reject;
});

export function startEngine(site: string, settings: Record<string, unknown>) {
  navigator.locks.request(LOCK, { ifAvailable: true }, (lock) => {
    if (lock) return becomeLeader(site, settings);
    // another tab runs the engine, this one waits in line to take over when that tab closes
    call({ type: 'ping' }).then(markReady, markFailed);
    navigator.locks.request(LOCK, () => becomeLeader(site, settings));
  });
  return ready;
}

// the returned promise never settles, so the tab keeps the lock until it closes
function becomeLeader(site: string, settings: Record<string, unknown>) {
  startWorker(site, settings);
  call({ type: 'ping' }).then(() => {
    markReady();
    resendUnanswered();
    channel.postMessage({ type: 'leader' } satisfies TabMessage);
  }, markFailed);
  return new Promise<never>(() => {});
}

export function sendSettings(settings: Record<string, unknown>) {
  if (worker) worker.postMessage({ type: 'settings', settings } satisfies Request);
  else channel.postMessage({ type: 'settings', settings } satisfies TabMessage);
}

export const invoke = <T>(command: string, args?: Record<string, unknown>) => call<T>({ type: 'invoke', command, args });

export const exportBackup = (settings: Record<string, unknown>) => call<Uint8Array>({ type: 'export', settings });

export const importBackup = (bytes: Uint8Array) => call<Record<string, unknown> | null>({ type: 'import', bytes });

export function listen<T>(event: string, listener: (payload: T) => void) {
  const set = listeners.get(event) ?? new Set();
  set.add(listener as (payload: unknown) => void);
  listeners.set(event, set);
  return () => set.delete(listener as (payload: unknown) => void);
}
