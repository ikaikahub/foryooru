import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [sveltekit(), tailwindcss()],
  // the engine's wasm and worker are prebuilt into src/lib/engine, vite only bundles them
  worker: { format: 'es' },
});
