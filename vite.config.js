import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from '@tailwindcss/vite'
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error node builtins are untyped here
import { execSync } from "node:child_process";

const host = process.env.TAURI_DEV_HOST;

// reads as 'v34 8b6e960' on a release, '#42 8b6e960' on a manual CI run and 'dev 8b6e960+' when the tree is dirty
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
  let build = 'dev';
  if (process.env.GITHUB_REF_TYPE === 'tag') build = process.env.GITHUB_REF_NAME;
  else if (process.env.GITHUB_RUN_NUMBER) build = `#${process.env.GITHUB_RUN_NUMBER}`;
  return [build, commit].filter(Boolean).join(' ');
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [
    sveltekit(),
    tailwindcss(),
  ],

  define: { __BUILD__: JSON.stringify(buildLabel()) },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1430,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1431,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
