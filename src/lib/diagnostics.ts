import { invoke } from "@tauri-apps/api/core";

type DiagnosticLevel = "info" | "warning" | "error";

const sessionId = crypto.randomUUID();
const sensitiveKey = /auth|token|secret|password|credential|cookie|session|prompt|body|content|text|key/i;
const originalConsole = {
  error: console.error.bind(console),
  warn: console.warn.bind(console),
};

function isDesktopApp() {
  return "__TAURI_INTERNALS__" in window;
}

function clip(value: unknown, limit = 240) {
  return String(value ?? "").replace(/\s+/g, " ").trim().slice(0, limit);
}

function sanitize(value: unknown, key = "", depth = 0): unknown {
  if (sensitiveKey.test(key)) {
    return typeof value === "string" ? `[redacted:${value.length}]` : "[redacted]";
  }
  if (depth > 5) return "[max-depth]";
  if (value instanceof Error) {
    return {
      name: value.name,
      message: clip(value.message, 500),
      stack: clip(value.stack, 1200),
    };
  }
  if (typeof value === "string") return clip(value, 500);
  if (typeof value === "number" || typeof value === "boolean" || value == null) return value;
  if (Array.isArray(value)) return value.slice(0, 30).map((item) => sanitize(item, key, depth + 1));
  if (typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .slice(0, 50)
        .map(([childKey, childValue]) => [childKey, sanitize(childValue, childKey, depth + 1)]),
    );
  }
  return clip(value);
}

function describeTarget(target: EventTarget | null) {
  if (!(target instanceof Element)) return {};
  const element = target.closest<HTMLElement>(
    "button, a, input, textarea, select, [role='button'], [role='option'], [role='tab'], [data-testid]",
  ) ?? target;
  const input = element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement
    ? element
    : null;

  return {
    tag: element.tagName.toLowerCase(),
    role: element.getAttribute("role"),
    name: input?.getAttribute("name"),
    inputType: input?.getAttribute("type"),
    ariaLabel: clip(element.getAttribute("aria-label"), 120),
    title: clip(element.getAttribute("title"), 120),
    placeholder: clip(input?.getAttribute("placeholder"), 120),
    label: input ? undefined : clip(element.textContent, 160),
    valueLength: input ? input.value.length : undefined,
    disabled: "disabled" in element ? Boolean((element as HTMLButtonElement).disabled) : undefined,
  };
}

export function recordDiagnostic(type: string, details: Record<string, unknown> = {}, level: DiagnosticLevel = "info") {
  if (!isDesktopApp()) return;
  const event = {
    sessionId,
    recordedAt: new Date().toISOString(),
    level,
    type,
    details: sanitize(details),
  };
  void invoke("record_diagnostic_event", { event }).catch(() => undefined);
}

export function summarizeForDiagnostic(value: unknown) {
  return sanitize(value);
}

export function installDiagnostics() {
  const diagnosticWindow = window as Window & { __flowContentDiagnosticsInstalled?: boolean };
  if (diagnosticWindow.__flowContentDiagnosticsInstalled) return;
  diagnosticWindow.__flowContentDiagnosticsInstalled = true;

  document.addEventListener("click", (event) => recordDiagnostic("ui-click", describeTarget(event.target)), true);
  document.addEventListener("change", (event) => recordDiagnostic("ui-change", describeTarget(event.target)), true);
  document.addEventListener("submit", (event) => recordDiagnostic("ui-submit", describeTarget(event.target)), true);
  document.addEventListener("visibilitychange", () => {
    recordDiagnostic("app-visibility", { state: document.visibilityState });
  });
  window.addEventListener("focus", () => recordDiagnostic("app-focus"));
  window.addEventListener("blur", () => recordDiagnostic("app-blur"));
  window.addEventListener("error", (event) => {
    recordDiagnostic("frontend-error", {
      message: event.message,
      filename: event.filename,
      line: event.lineno,
      column: event.colno,
      error: event.error,
    }, "error");
  });
  window.addEventListener("unhandledrejection", (event) => {
    recordDiagnostic("unhandled-rejection", { reason: event.reason }, "error");
  });

  console.error = (...args: unknown[]) => {
    recordDiagnostic("console-error", { args }, "error");
    originalConsole.error(...args);
  };
  console.warn = (...args: unknown[]) => {
    recordDiagnostic("console-warning", { args }, "warning");
    originalConsole.warn(...args);
  };

  recordDiagnostic("diagnostic-session-start", {
    href: location.href,
    viewport: { width: innerWidth, height: innerHeight },
  });
}
