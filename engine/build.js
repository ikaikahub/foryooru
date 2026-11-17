// Builds the Rust engine to WebAssembly and generates its JS bindings into src/lib/engine/pkg.
// SQLite is compiled from C for the browser, which needs a clang that can target wasm.
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const engine = dirname(fileURLToPath(import.meta.url));
const output = join(engine, '..', 'src', 'lib', 'engine', 'pkg');
const env = { ...process.env };

// on Windows: a portable wasi-sdk in ~/tools needs no installer, an LLVM install works too.
// CI and other systems use whatever clang is already on the PATH
const toolFolders = [join(homedir(), 'tools', 'wasi-sdk-34.0-x86_64-windows', 'bin'), 'C:\\Program Files\\LLVM\\bin'];
const tools = process.platform === 'win32' ? toolFolders.find((folder) => existsSync(join(folder, 'clang.exe'))) : undefined;
if (tools) {
  env.PATH = `${tools}${delimiter}${env.PATH}`;
  env.CC_wasm32_unknown_unknown = join(tools, 'clang.exe');
  env.AR_wasm32_unknown_unknown = join(tools, 'llvm-ar.exe');
}

const run = (command, args) => execFileSync(command, args, { cwd: engine, env, stdio: 'inherit' });

run('cargo', ['build', '-p', 'foryooru-web', '--target', 'wasm32-unknown-unknown', '--release']);
run('wasm-bindgen', [
  join(engine, 'target', 'wasm32-unknown-unknown', 'release', 'foryooru_web.wasm'),
  '--target',
  'web',
  '--out-dir',
  output,
]);
