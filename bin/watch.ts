import fs from "fs/promises";
import child_process from "child_process";
import path from "path";
import process from "process";
import * as pty from "node-pty";

const BUILD_CMD = "cargo build --color always";
const BUILD_DIR = path.join(process.cwd(), "server");
const EXE_NAME = process.platform === "win32" ? "orchid-ls.exe" : "orchid-ls"
const EXE_PATH = path.join(process.cwd(), "server", "target", "debug", EXE_NAME);
const EXE_DEST = path.join(process.cwd(), "client", "public", EXE_NAME);

let serverAc = new AbortController();

async function tryBuild() {
  serverAc.abort("stale");
  serverAc = new AbortController();
  const child = child_process.exec(BUILD_CMD, {
    cwd: BUILD_DIR,
    signal: serverAc.signal,
  });
  child.stdout?.on("data", d => process.stdout.write(d));
  child.stderr?.on("data", d => process.stderr.write(d));
  await new Promise(r => child.on("exit", r));
  if (child.exitCode !== 0 || serverAc.signal.aborted) return;
  await fs.cp(EXE_PATH, EXE_DEST);
}

async function watchServer() {
  for await (const ev of fs.watch("./server", { recursive: true, persistent: true })) {
    if (!ev.filename) continue;
    const ext = ev.filename.split(".").pop();
    if (!ext || !["rs", "toml"].includes(ext)) continue;
    console.log("Files changed, rebuilding...");
    tryBuild();
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
  serverAc.abort("exiting");
  clientAc.abort("exiting");
  await new Promise(r => setTimeout(r, 100));
  process.exit(0);
}

await startClientWatcher();
await tryBuild();

console.log("Watching server sources. Press 'q' quit, `r` to reload");

process.stdin.setRawMode(true);
process.stdin.on("data", async data => {
  const cmd = data.toString("utf8");
  if (cmd === "q") await die();
  else if (cmd === "r") await tryBuild();
  // ^C and ^D produce this in raw mode for some reason
  else if (data.length === 1 && [3, 4].includes(data.readUint8(0))) await die();
  else console.log(`Unrecognized command "${cmd}".`);
})

await watchServer();
