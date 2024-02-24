import fs from "fs/promises";
import fscb from "fs";
import child_process from "child_process";
import path from "path";
import process from "process";
import * as pty from "node-pty";

const CARGO_CMD = "cargo build --color always";
const SERVER_DIR = path.join(process.cwd(), "server");
const ORCHID_DIR = path.join(path.dirname(process.cwd()), "orchid");
const EXE_NAME = process.platform === "win32" ? "orchid-ls.exe" : "orchid-ls"
const EXE_PATH = path.join(process.cwd(), "server", "target", "debug", EXE_NAME);
const EXE_DEST = path.join(process.cwd(), "client", "public", EXE_NAME);

interface Builder {
  abort(): void,
  build(): Promise<boolean>
}

function getBuilder(build_dir: string, build_cmd: string): Builder {
  let ac = new AbortController();
  return {
    abort: () => ac.abort("exiting"),
    async build() {
      ac.abort("stale");
      ac = new AbortController();
      const child = child_process.exec(build_cmd, {
        cwd: build_dir,
        signal: ac.signal,
      });
      child.stdout?.on("data", d => process.stdout.write(d));
      child.stderr?.on("data", d => process.stderr.write(d));
      await new Promise(r => child.on("exit", r));
      if (child.exitCode !== 0 && !ac.signal.aborted) {
        throw new Error(`Build for ${path.basename(build_dir)} failed`);
      }
      return !ac.signal.aborted;
    }
  }
}

let server_builder = getBuilder(SERVER_DIR, CARGO_CMD);
async function buildServer() {
  if (!await server_builder.build()) return;
  await fs.cp(EXE_PATH, EXE_DEST);
}
let orchid_builder = getBuilder(ORCHID_DIR, CARGO_CMD);
async function buildOrchid() {
  if (!await orchid_builder.build()) return;
  await buildServer();
}

async function* watchRustProject(path: string): AsyncGenerator<undefined, undefined, undefined> {
  while (true) {
    try {
      for await (const chg of fs.watch(path, { recursive: true, persistent: true })) {
        if (!chg.filename) continue;
        const ext = chg.filename.split(".").pop();
        if (ext !== undefined && ["rs", "toml"].includes(ext)) yield;
      }
    } catch (e) {
      // this happens sometimes when Cargo instances interfere with each other
      if (e.code === "ENOENT" && fscb.existsSync(path)) continue;
      throw e;
    }
  }
}

async function watchServer() {
  for await (const _ of watchRustProject("./server")) {
    console.log("Server changed, rebuilding...");
    buildServer().catch(() => {});
  }
}

async function watchOrchid() {
  for await (const _ of watchRustProject("../orchid")) {
    console.log("Orchid changed, rebuilding...")
    buildOrchid().catch(() => {})
  }
}

let clientAc = new AbortController();

async function startClientWatcher() {
  const shell = process.platform === 'win32' ? 'powershell.exe' : 'bash';
  const ptyProc = pty.spawn(shell, ["-c", "npm run watch:client"], {
    name: "xterm-256color",
    cols: 80,
    rows: 30,
    cwd: path.join(process.cwd(), "client"),
    env: process.env,
  });
  ptyProc.onData(d => process.stdout.write(d));
  clientAc.signal.onabort = () => ptyProc.kill();
  await new Promise<void>(r => ptyProc.onData(d => {
    if (d.includes("Watching for file changes")) r()
  }));
}

async function die() {
  console.log("Exiting...");
  clientAc.abort("exiting");
  server_builder.abort();
  orchid_builder.abort();
  await new Promise(r => setTimeout(r, 100));
  process.exit(0);
}

if (!process.argv.includes("--no-client")) await startClientWatcher();
else console.log("Skipping client");
await buildOrchid();

console.log("Watching server sources. Press 'q' quit, `r` to reload");

process.stdin.setRawMode(true);
process.stdin.on("data", async data => {
  const cmd = data.toString("utf8");
  if (cmd === "q") await die();
  else if (cmd === "r") await buildOrchid();
  // ^C and ^D produce this in raw mode for some reason
  else if (data.length === 1 && [3, 4].includes(data.readUint8(0))) await die();
  else console.log(`Unrecognized command "${cmd}".`);
})

await Promise.allSettled([
  watchServer(),
  watchOrchid(),
]);
