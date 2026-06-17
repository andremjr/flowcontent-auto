import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const sessionId = new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-");
const sessionDir = path.resolve("captures", `test-session-${sessionId}`);
const logDir = path.join(sessionDir, "logs");
fs.mkdirSync(logDir, { recursive: true });

const diagnosticFile = process.env.FLOW_DIAGNOSTICS_FILE
  || path.join(process.env.APPDATA || "", "com.flowcontent.auto", "diagnostics", "manual-test.ndjson");
const sessionDiagnosticFile = path.join(sessionDir, "app-events.ndjson");
const children = [];
let diagnosticOffset = fs.existsSync(diagnosticFile) ? fs.statSync(diagnosticFile).size : 0;
let terminalOpen = true;
process.stdout.on("error", (error) => {
  if (error.code === "EPIPE") terminalOpen = false;
});
process.stderr.on("error", (error) => {
  if (error.code === "EPIPE") terminalOpen = false;
});

function terminalWrite(stream, value) {
  if (!terminalOpen || stream.destroyed) return;
  try {
    stream.write(value);
  } catch (error) {
    if (error.code === "EPIPE") terminalOpen = false;
    else throw error;
  }
}

function writeManifest(status, extra = {}) {
  fs.writeFileSync(path.join(sessionDir, "session.json"), JSON.stringify({
    status,
    startedAt: sessionId,
    updatedAt: new Date().toISOString(),
    diagnosticFile,
    sessionDir,
    ...extra,
  }, null, 2));
}

function start(name, command, args, env = {}) {
  const stdout = fs.createWriteStream(path.join(logDir, `${name}.stdout.log`), { flags: "a" });
  const stderr = fs.createWriteStream(path.join(logDir, `${name}.stderr.log`), { flags: "a" });
  const child = spawn(command, args, {
    cwd: root,
    env: { ...process.env, ...env },
    shell: process.platform === "win32",
    windowsHide: true,
  });
  child.stdout?.on("data", (chunk) => {
    stdout.write(chunk);
    terminalWrite(process.stdout, `[${name}] ${chunk}`);
  });
  child.stderr?.on("data", (chunk) => {
    stderr.write(chunk);
    terminalWrite(process.stderr, `[${name}] ${chunk}`);
  });
  child.on("exit", (code, signal) => {
    stdout.end();
    stderr.end();
    terminalWrite(process.stdout, `[${name}] encerrado (${signal ?? code})\n`);
  });
  children.push(child);
  return child;
}

function copyNewDiagnostics() {
  if (!fs.existsSync(diagnosticFile)) return;
  const size = fs.statSync(diagnosticFile).size;
  if (size < diagnosticOffset) diagnosticOffset = 0;
  if (size === diagnosticOffset) return;
  const input = fs.openSync(diagnosticFile, "r");
  const buffer = Buffer.alloc(size - diagnosticOffset);
  fs.readSync(input, buffer, 0, buffer.length, diagnosticOffset);
  fs.closeSync(input);
  fs.appendFileSync(sessionDiagnosticFile, buffer);
  diagnosticOffset = size;
}

writeManifest("running");
console.log(`Sessao de teste: ${sessionDir}`);
console.log("Use o aplicativo normalmente. Pressione Ctrl+C ao terminar.");

start("diagnostics", "node", ["tools/watch-diagnostics.mjs"], { FLOW_DIAGNOSTICS_FILE: diagnosticFile });
start("flow-recorder", "node", ["--experimental-websocket", "tools/flow-recorder.mjs"], { FLOW_CAPTURE_DIR: sessionDir });
const app = start("tauri", "npm", ["run", "tauri:dev"]);
const diagnosticTimer = setInterval(copyNewDiagnostics, 500);

let stopping = false;
function stop(signal) {
  if (stopping) return;
  stopping = true;
  clearInterval(diagnosticTimer);
  copyNewDiagnostics();
  writeManifest("stopping", { signal });
  for (const child of children) {
    if (child.killed) continue;
    if (process.platform === "win32") {
      spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { windowsHide: true });
    } else {
      child.kill("SIGTERM");
    }
  }
  setTimeout(() => {
    writeManifest("stopped", { signal, stoppedAt: new Date().toISOString() });
    process.exit(0);
  }, 1500).unref();
}

app.on("exit", (code) => {
  if (!stopping) stop(code === 0 ? "tauri-exit" : `tauri-failed-${code}`);
});
process.on("SIGINT", () => stop("SIGINT"));
process.on("SIGTERM", () => stop("SIGTERM"));
