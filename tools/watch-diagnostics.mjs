import fs from "node:fs";
import path from "node:path";

const diagnosticFile = process.env.FLOW_DIAGNOSTICS_FILE
  || path.join(process.env.APPDATA || "", "com.flowcontent.auto", "diagnostics", "manual-test.ndjson");
const showAll = process.argv.includes("--all");
let offset = 0;
let remainder = "";
let announcedWaiting = false;

function display(event) {
  const time = event.recordedAt?.slice(11, 19) || event.serverRecordedAt || "unknown";
  const details = event.details || {};
  const subject = details.command
    || details.label
    || details.message
    || details.error?.message
    || details.kind
    || "";
  const duration = details.durationMs == null ? "" : ` ${details.durationMs}ms`;
  const level = String(event.level || "info").toUpperCase().padEnd(7);
  console.log(`${time} ${level} ${event.type}${duration}${subject ? ` | ${subject}` : ""}`);
}

function readNewEvents() {
  if (!fs.existsSync(diagnosticFile)) {
    if (!announcedWaiting) {
      console.log(`Aguardando eventos em ${diagnosticFile}`);
      announcedWaiting = true;
    }
    return;
  }

  const size = fs.statSync(diagnosticFile).size;
  if (size < offset) {
    offset = 0;
    remainder = "";
  }
  if (size === offset) return;

  const length = size - offset;
  const buffer = Buffer.alloc(length);
  const descriptor = fs.openSync(diagnosticFile, "r");
  fs.readSync(descriptor, buffer, 0, length, offset);
  fs.closeSync(descriptor);
  offset = size;

  const lines = `${remainder}${buffer.toString("utf8")}`.split(/\r?\n/);
  remainder = lines.pop() || "";
  for (const line of lines) {
    if (!line.trim()) continue;
    try {
      display(JSON.parse(line));
    } catch {
      console.log(`INVALID ${line.slice(0, 200)}`);
    }
  }
}

if (fs.existsSync(diagnosticFile) && !showAll) {
  offset = fs.statSync(diagnosticFile).size;
}
console.log(`Monitorando ${diagnosticFile}${showAll ? " desde o inicio" : ""}`);
readNewEvents();
setInterval(readNewEvents, 500);

