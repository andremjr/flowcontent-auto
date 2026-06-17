import fs from "node:fs";
import path from "node:path";

const DEBUG_URL = process.env.FLOW_DEBUG_URL || "http://127.0.0.1:9223";
const OUTPUT_ROOT = process.env.FLOW_CAPTURE_DIR || path.resolve("captures");
const SCREENSHOT_INTERVAL_MS = 3000;
const SNAPSHOT_INTERVAL_MS = 5000;

const sessionId = new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-");
const sessionDir = path.join(OUTPUT_ROOT, `flow-session-${sessionId}`);
const screenshotDir = path.join(sessionDir, "screenshots");
const snapshotDir = path.join(sessionDir, "snapshots");
const networkDir = path.join(sessionDir, "network");
fs.mkdirSync(screenshotDir, { recursive: true });
fs.mkdirSync(snapshotDir, { recursive: true });
fs.mkdirSync(networkDir, { recursive: true });

const eventFile = path.join(sessionDir, "events.ndjson");
const statusFile = path.join(sessionDir, "status.json");
let sequence = 0;
let socket;
let requestId = 0;
let currentTargetId;
let currentUrl;
const pending = new Map();
const networkRequests = new Map();
const ignoredNetworkHosts = new Set([
  "www.google-analytics.com",
  "analytics.google.com",
]);

function redact(value) {
  if (typeof value !== "string") return value;
  const redacted = value
    .replace(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi, "[email-redacted]")
    .replace(/\bya29\.[A-Za-z0-9_-]+\b/g, "[token-redacted]")
    .replace(/\bBearer\s+[A-Za-z0-9._~-]+\b/gi, "Bearer [redacted]");
  if (/^https?:\/\//i.test(redacted)) return sanitizeUrl(redacted);
  return redacted;
}

function sanitizeUrl(rawUrl) {
  try {
    const url = new URL(rawUrl);
    for (const key of [...url.searchParams.keys()]) {
      url.searchParams.set(key, "[redacted]");
    }
    return url.toString();
  } catch {
    return rawUrl;
  }
}

function sanitizeStructured(value, depth = 0) {
  if (depth > 8) return "[max-depth]";
  if (Array.isArray(value)) {
    return value.slice(0, 50).map((item) => sanitizeStructured(item, depth + 1));
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).slice(0, 100).map(([key, item]) => {
        if (/auth|token|cookie|session|credential|secret|password|email|account|api.?key/i.test(key)) {
          return [key, "[redacted]"];
        }
        return [key, sanitizeStructured(item, depth + 1)];
      }),
    );
  }
  return redact(value);
}

function sanitizeBody(rawBody, mimeType = "") {
  if (!rawBody) return undefined;
  if (rawBody.length > 250_000) return { omitted: true, reason: "body-too-large", length: rawBody.length };
  if (mimeType.includes("json") || rawBody.trim().startsWith("{") || rawBody.trim().startsWith("[")) {
    try {
      return sanitizeStructured(JSON.parse(rawBody));
    } catch {
      return { parseError: true, preview: redact(rawBody.slice(0, 2000)) };
    }
  }
  return { omitted: true, reason: "non-json-body", length: rawBody.length };
}

function shouldCaptureNetworkUrl(rawUrl) {
  try {
    const url = new URL(rawUrl);
    if (ignoredNetworkHosts.has(url.hostname)) return false;
    if (url.pathname === "/api/auth/session") return false;
    return true;
  } catch {
    return false;
  }
}

function writeNetworkRecord(requestId, record) {
  const filename = `${String(sequence + 1).padStart(6, "0")}-${requestId.replaceAll(".", "_")}.json`;
  fs.writeFileSync(path.join(networkDir, filename), JSON.stringify(record, null, 2));
  writeEvent("network-record", {
    filename,
    method: record.request?.method,
    url: record.request?.url,
    status: record.response?.status,
  });
}

function writeEvent(type, payload = {}) {
  const event = {
    sequence: ++sequence,
    recordedAt: new Date().toISOString(),
    type,
    payload: JSON.parse(JSON.stringify(payload, (_, value) => redact(value))),
  };
  fs.appendFileSync(eventFile, `${JSON.stringify(event)}\n`);
}

function writeStatus(status, extra = {}) {
  fs.writeFileSync(
    statusFile,
    JSON.stringify(
      {
        status,
        updatedAt: new Date().toISOString(),
        targetId: currentTargetId,
        url: redact(currentUrl),
        ...extra,
      },
      null,
      2,
    ),
  );
}

async function findFlowTarget() {
  const targets = await (await fetch(`${DEBUG_URL}/json/list`)).json();
  return targets.find(
    (target) =>
      target.type === "page" &&
      (target.url.includes("labs.google") || target.url.includes("flow.google")),
  );
}

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++requestId;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function installObserver() {
  const source = `(() => {
    if (window.__flowContentAutoObserverInstalled) return;
    window.__flowContentAutoObserverInstalled = true;
    window.__flowContentAutoEvents = [];

    const clip = (value, limit = 180) =>
      String(value || "").replace(/\\s+/g, " ").trim().slice(0, limit);
    const describe = (element) => {
      if (!(element instanceof Element)) return {};
      const input = element.closest("input, textarea, select");
      const interactive = element.closest("button, a, [role='button'], [role='option'], [role='tab']");
      const target = input || interactive || element;
      return {
        tag: target.tagName?.toLowerCase(),
        role: target.getAttribute?.("role"),
        ariaLabel: clip(target.getAttribute?.("aria-label")),
        title: clip(target.getAttribute?.("title")),
        text: clip(target.innerText || target.textContent),
        placeholder: clip(input?.getAttribute?.("placeholder")),
        inputType: input?.getAttribute?.("type"),
        valueLength: input && input.type !== "password" ? String(input.value || "").length : undefined,
      };
    };
    const record = (type, details) => {
      window.__flowContentAutoEvents.push({
        type,
        at: new Date().toISOString(),
        details,
      });
      if (window.__flowContentAutoEvents.length > 1000) {
        window.__flowContentAutoEvents.splice(0, 250);
      }
    };

    document.addEventListener("click", (event) => record("click", describe(event.target)), true);
    document.addEventListener("change", (event) => record("change", describe(event.target)), true);
    document.addEventListener("submit", (event) => record("submit", describe(event.target)), true);
    window.addEventListener("popstate", () => record("navigation", { url: location.href }));
    window.addEventListener("hashchange", () => record("navigation", { url: location.href }));

    let mutationCount = 0;
    let mutationTimer;
    const observer = new MutationObserver((mutations) => {
      mutationCount += mutations.length;
      clearTimeout(mutationTimer);
      mutationTimer = setTimeout(() => {
        record("dom-change", {
          mutationCount,
          title: document.title,
          visibleTextLength: document.body?.innerText?.length || 0,
        });
        mutationCount = 0;
      }, 500);
    });
    observer.observe(document.documentElement, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["aria-label", "aria-selected", "aria-expanded", "disabled", "data-state"],
    });
    record("observer-ready", { url: location.href, title: document.title });
  })();`;

  await send("Page.addScriptToEvaluateOnNewDocument", { source });
  await send("Runtime.evaluate", { expression: source });
}

async function drainBrowserEvents() {
  const result = await send("Runtime.evaluate", {
    expression:
      "JSON.stringify((window.__flowContentAutoEvents || []).splice(0, window.__flowContentAutoEvents?.length || 0))",
    returnByValue: true,
  });
  const raw = result?.result?.result?.value;
  if (!raw) return;
  for (const browserEvent of JSON.parse(raw)) {
    writeEvent("browser-event", browserEvent);
  }
}

async function captureScreenshot() {
  const result = await send("Page.captureScreenshot", {
    format: "jpeg",
    quality: 70,
    captureBeyondViewport: false,
  });
  if (!result?.result?.data) return;
  const filename = `${String(sequence + 1).padStart(6, "0")}.jpg`;
  fs.writeFileSync(path.join(screenshotDir, filename), Buffer.from(result.result.data, "base64"));
  writeEvent("screenshot", { filename });
}

async function captureSnapshot() {
  const result = await send("Runtime.evaluate", {
    expression: `JSON.stringify({
      url: location.href,
      title: document.title,
      viewport: { width: innerWidth, height: innerHeight },
      text: document.body?.innerText || "",
      controls: [...document.querySelectorAll("button, a, input, textarea, select, [role='button'], [role='option'], [role='tab']")]
        .slice(0, 500)
        .map((element) => ({
          tag: element.tagName.toLowerCase(),
          role: element.getAttribute("role"),
          ariaLabel: element.getAttribute("aria-label"),
          title: element.getAttribute("title"),
          placeholder: element.getAttribute("placeholder"),
          inputType: element.getAttribute("type"),
          text: (element.innerText || element.textContent || "").replace(/\\s+/g, " ").trim().slice(0, 240),
          disabled: Boolean(element.disabled),
        })),
    })`,
    returnByValue: true,
  });
  const raw = result?.result?.result?.value;
  if (!raw) return;
  const snapshot = JSON.parse(raw);
  const filename = `${String(sequence + 1).padStart(6, "0")}.json`;
  fs.writeFileSync(
    path.join(snapshotDir, filename),
    JSON.stringify(JSON.parse(JSON.stringify(snapshot, (_, value) => redact(value))), null, 2),
  );
  writeEvent("snapshot", { filename, title: snapshot.title, url: snapshot.url });
}

async function connect() {
  const target = await findFlowTarget();
  if (!target) throw new Error("Nenhuma aba Google Flow encontrada no canal de depuração.");
  currentTargetId = target.id;
  currentUrl = target.url;
  socket = new WebSocket(target.webSocketDebuggerUrl);

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const handler = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) handler.reject(new Error(message.error.message));
      else handler.resolve(message);
      return;
    }
    if (message.method === "Page.frameNavigated" && !message.params.frame.parentId) {
      currentUrl = message.params.frame.url;
      writeEvent("navigation", { url: currentUrl });
      writeStatus("recording");
    }
    if (message.method === "Runtime.consoleAPICalled") {
      writeEvent("browser-console", {
        level: message.params.type,
        values: message.params.args?.slice(0, 10).map((arg) => arg.value ?? arg.description ?? arg.type),
      });
    }
    if (message.method === "Runtime.exceptionThrown") {
      writeEvent("browser-exception", {
        text: message.params.exceptionDetails?.text,
        lineNumber: message.params.exceptionDetails?.lineNumber,
        columnNumber: message.params.exceptionDetails?.columnNumber,
        exception: message.params.exceptionDetails?.exception?.description,
      });
    }
    if (message.method === "Network.requestWillBeSent") {
      const { requestId, request, type, initiator, timestamp } = message.params;
      if (!["Fetch", "XHR"].includes(type)) return;
      if (!shouldCaptureNetworkUrl(request.url)) return;
      networkRequests.set(requestId, {
        request: {
          method: request.method,
          url: sanitizeUrl(request.url),
          type,
          timestamp,
          initiatorType: initiator?.type,
          body: sanitizeBody(request.postData, request.headers?.["content-type"] || ""),
        },
      });
    }
    if (message.method === "Network.responseReceived") {
      const { requestId, response, type, timestamp } = message.params;
      const record = networkRequests.get(requestId);
      if (!record || !["Fetch", "XHR"].includes(type)) return;
      record.response = {
        status: response.status,
        statusText: response.statusText,
        url: sanitizeUrl(response.url),
        mimeType: response.mimeType,
        protocol: response.protocol,
        fromDiskCache: response.fromDiskCache,
        fromServiceWorker: response.fromServiceWorker,
        timestamp,
      };
    }
    if (message.method === "Network.loadingFinished") {
      const { requestId, encodedDataLength } = message.params;
      const record = networkRequests.get(requestId);
      if (!record) return;
      record.response ??= {};
      record.response.encodedDataLength = encodedDataLength;
      send("Network.getResponseBody", { requestId })
        .then((result) => {
          const body = result?.result?.body;
          record.response.body = sanitizeBody(body, record.response.mimeType || "");
          writeNetworkRecord(requestId, record);
          networkRequests.delete(requestId);
        })
        .catch(() => {
          writeNetworkRecord(requestId, record);
          networkRequests.delete(requestId);
        });
    }
    if (message.method === "Network.loadingFailed") {
      const { requestId, errorText, canceled } = message.params;
      const record = networkRequests.get(requestId);
      if (!record) return;
      record.failure = { errorText, canceled };
      writeNetworkRecord(requestId, record);
      networkRequests.delete(requestId);
    }
  });

  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Network.enable", {
    maxTotalBufferSize: 20_000_000,
    maxResourceBufferSize: 2_000_000,
    maxPostDataSize: 250_000,
  });
  await installObserver();
  await captureSnapshot();
  await captureScreenshot();
  writeEvent("recorder-started", { targetId: currentTargetId, url: currentUrl });
  writeStatus("recording", { sessionDir });
}

let screenshotTimer;
let snapshotTimer;
let eventTimer;

async function start() {
  writeStatus("waiting-for-flow", { debugUrl: DEBUG_URL, sessionDir });
  while (!socket) {
    try {
      await connect();
    } catch (error) {
      writeStatus("waiting-for-flow", { debugUrl: DEBUG_URL, sessionDir, warning: error.message });
      await new Promise((resolve) => setTimeout(resolve, 2000));
    }
  }
  screenshotTimer = setInterval(() => captureScreenshot().catch(handleRecoverableError), SCREENSHOT_INTERVAL_MS);
  snapshotTimer = setInterval(() => captureSnapshot().catch(handleRecoverableError), SNAPSHOT_INTERVAL_MS);
  eventTimer = setInterval(() => drainBrowserEvents().catch(handleRecoverableError), 1000);
}

function handleRecoverableError(error) {
  writeEvent("recorder-warning", { message: error.message });
  writeStatus("recording-with-warning", { warning: error.message });
}

async function stop(signal) {
  clearInterval(screenshotTimer);
  clearInterval(snapshotTimer);
  clearInterval(eventTimer);
  try {
    await drainBrowserEvents();
    await captureSnapshot();
    await captureScreenshot();
  } catch {
    // Best-effort final capture.
  }
  writeEvent("recorder-stopped", { signal });
  writeStatus("stopped", { signal });
  socket?.close();
  process.exit(0);
}

process.on("SIGINT", () => stop("SIGINT"));
process.on("SIGTERM", () => stop("SIGTERM"));

start().catch((error) => {
  writeEvent("recorder-failed", { message: error.message, stack: error.stack });
  writeStatus("failed", { error: error.message });
  process.exit(1);
});
