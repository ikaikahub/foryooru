import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
// @ts-ignore -- node builtins are untyped here, the app's tsconfig has no @types/node
import { execSync } from 'node:child_process';

/** @type {{ GITHUB_RUN_NUMBER?: string }} */
const env = /** @type {any} */ (globalThis).process?.env ?? {};

// reads as '#42 8b6e960' on a deployed build and 'dev 8b6e960+' when the tree is dirty
function buildLabel() {
  let commit = '';
  try {
    /** @param {string} command */
    const git = (command) => execSync(command, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
    commit = git('git rev-parse --short HEAD');
    if (git('git status --porcelain')) commit += '+';
  } catch {
    // no git available, the run number still identifies a deploy
  }
  return [env.GITHUB_RUN_NUMBER ? `#${env.GITHUB_RUN_NUMBER}` : 'dev', commit].filter(Boolean).join(' ');
}

export default defineConfig({
  plugins: [sveltekit(), tailwindcss()],
  // the engine's wasm and worker are prebuilt into src/lib/engine, vite only bundles them
  worker: { format: 'es' },
  define: { __BUILD__: JSON.stringify(buildLabel()) },
});
