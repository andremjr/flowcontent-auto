import fs from "node:fs";
import path from "node:path";

const capturesRoot = path.resolve("captures");
const requested = process.argv[2] ? path.resolve(process.argv[2]) : null;
const sessionDir = requested || fs.readdirSync(capturesRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name.startsWith("test-session-"))
  .map((entry) => path.join(capturesRoot, entry.name))
  .sort()
  .at(-1);

if (!sessionDir || !fs.existsSync(sessionDir)) {
  console.error("Nenhuma sessao monitorada encontrada.");
  process.exit(1);
}

function readNdjson(filename) {
  if (!fs.existsSync(filename)) return [];
  return fs.readFileSync(filename, "utf8").split(/\r?\n/).filter(Boolean).flatMap((line) => {
    try { return [JSON.parse(line)]; } catch { return []; }
  });
}

const appEvents = readNdjson(path.join(sessionDir, "app-events.ndjson"));
const flowSession = fs.readdirSync(sessionDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name.startsWith("flow-session-"))
  .map((entry) => path.join(sessionDir, entry.name))
  .sort()
  .at(-1);
const browserEvents = flowSession ? readNdjson(path.join(flowSession, "events.ndjson")) : [];
const networkCount = flowSession && fs.existsSync(path.join(flowSession, "network"))
  ? fs.readdirSync(path.join(flowSession, "network")).length
  : 0;

const report = {
  sessionDir,
  generatedAt: new Date().toISOString(),
  app: {
    events: appEvents.length,
    errors: appEvents.filter((event) => event.level === "error"),
    warnings: appEvents.filter((event) => event.level === "warning"),
    ipcErrors: appEvents.filter((event) => event.type === "ipc-error"),
  },
  browser: {
    flowSession: flowSession || null,
    events: browserEvents.length,
    exceptions: browserEvents.filter((event) => event.type === "browser-exception"),
    consoleErrors: browserEvents.filter((event) => event.type === "browser-console" && event.payload?.level === "error"),
    networkRecords: networkCount,
  },
};

const output = path.join(sessionDir, "report.json");
fs.writeFileSync(output, JSON.stringify(report, null, 2));
console.log(JSON.stringify({
  output,
  appEvents: report.app.events,
  appErrors: report.app.errors.length,
  appWarnings: report.app.warnings.length,
  browserEvents: report.browser.events,
  browserExceptions: report.browser.exceptions.length,
  networkRecords: report.browser.networkRecords,
}, null, 2));
