import assert from "node:assert/strict";
import fs from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const pageBridgeScript = await fs.readFile(new URL("../extension/page_bridge.js", import.meta.url), "utf8");

test("page bridge emits heartbeat and ready signal on boot", async () => {
  const postedMessages = [];
  const messageListeners = [];
  const intervalCallbacks = [];
  const location = {
    href: "https://labs.google/fx/pt/tools/flow",
    pathname: "/fx/pt/tools/flow",
  };

  const sessionStorage = new Map();
  const context = {
    console,
    location,
    document: {
      title: "Google Flow",
      querySelectorAll: () => [],
    },
    sessionStorage: {
      getItem(key) {
        return sessionStorage.has(key) ? sessionStorage.get(key) : null;
      },
      setItem(key, value) {
        sessionStorage.set(key, value);
      },
    },
    fetch: async () => {
      return {
        ok: true,
        status: 200,
        async json() {
          return { result: { data: { json: { project: { id: "985209ec-1e76-455c-b93b-88b6f8b60750" } } } } };
        },
      };
    },
    Headers: class {
      constructor() {
        this.map = new Map();
      }
      get(key) {
        return this.map.get(String(key).toLowerCase()) ?? null;
      }
      set(key, value) {
        this.map.set(String(key).toLowerCase(), value);
      }
    },
    XMLHttpRequest: class {},
    HTMLElement: class {},
    HTMLInputElement: class {},
    HTMLTextAreaElement: class {},
    InputEvent: class {},
    Event: class {},
    getComputedStyle: () => ({ display: "block", visibility: "visible" }),
    setInterval(callback) {
      intervalCallbacks.push(callback);
      return intervalCallbacks.length;
    },
    setTimeout,
    clearTimeout,
  };

  context.window = context;
  context.window.fetch = context.fetch;
  context.window.addEventListener = (type, listener) => {
    if (type === "message") {
      messageListeners.push(listener);
    }
  };
  context.window.postMessage = (message) => {
    postedMessages.push(message);
    for (const listener of messageListeners) {
      listener({ source: context.window, data: message });
    }
  };
  context.window.grecaptcha = {
    enterprise: {
      async execute() {
        return "recaptcha-token";
      },
    },
  };
  context.XMLHttpRequest.prototype.open = function () {};
  context.XMLHttpRequest.prototype.setRequestHeader = function () {};

  vm.runInNewContext(pageBridgeScript, context);
  await new Promise((resolve) => setTimeout(resolve, 20));

  const heartbeat = postedMessages.find(
    (message) => message.channel === "cf-from-bridge" && message.type === "FLOWCONTENT_HEARTBEAT",
  );
  assert.equal(heartbeat?.payload?.pageDetected, true);
  assert.equal(heartbeat?.payload?.projectId, null);

  const readySignal = postedMessages.find(
    (message) => message.channel === "cf-from-bridge" && message.type === "bridge-ready",
  );
  assert.ok(readySignal);
  assert.equal(intervalCallbacks.length, 1);
});
