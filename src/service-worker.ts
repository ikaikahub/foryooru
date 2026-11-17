/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />
/// <reference types="@sveltejs/kit" />

import { build, files, prerendered, version } from '$service-worker';

const sw = self as unknown as ServiceWorkerGlobalScope;

const CACHE = `cache-${version}`;
const APP_FILES = [...build, ...files, ...prerendered];

// a new version takes over straight away instead of waiting for every tab to close, the page then reloads into it
sw.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      // skip the browser's HTTP cache, GitHub Pages lets it keep pages for 10 minutes and the new version would get the old html
      .then((cache) => cache.addAll(APP_FILES.map((file) => new Request(file, { cache: 'reload' }))))
      .then(() => sw.skipWaiting()),
  );
});

sw.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => sw.clients.claim()),
  );
});

// the app itself comes from the cache, booru requests always go to the network
sw.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== location.origin) return;

  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      // hashed build files never change; ones SvelteKit doesn't list, like the engine's wasm in the worker, are kept once fetched
      const immutable = url.pathname.includes('/_app/immutable/');
      if (APP_FILES.includes(url.pathname) || immutable) {
        const cached = await cache.match(url.pathname);
        if (cached) return cached;
      }
      try {
        const response = await fetch(event.request);
        if (immutable && response.ok) await cache.put(url.pathname, response.clone());
        return response;
      } catch (error) {
        // offline navigation falls back to the cached page
        const page = event.request.mode === 'navigate' ? await cache.match(prerendered[0]) : undefined;
        if (page) return page;
        throw error;
      }
    })(),
  );
});
