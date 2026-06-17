// content.js - Isolated world relay entre page_bridge.js (MAIN) e background.js

(function () {
  const instanceId = crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`;
  window.__cfContentInstance = instanceId;

  let runtimeAvailable = true;
  let bridgeReady = false;
  const pendingFromBg = new Map();

  const isActiveInstance = () => window.__cfContentInstance === instanceId;

  function csLog(...args) {
    const ts = new Date().toISOString();
    console.log(`[CS][${ts}]`, ...args);
  }

  function csWarn(...args) {
    const ts = new Date().toISOString();
    console.warn(`[CS][${ts}]`, ...args);
  }

  function csError(...args) {
    const ts = new Date().toISOString();
    console.error(`[CS][${ts}]`, ...args);
  }

  function isExtensionContextInvalidated(err) {
    const message = err?.message || String(err);
    return message.includes("Extension context invalidated");
  }

  function invalidateRuntime(err) {
    if (!runtimeAvailable) return;
    runtimeAvailable = false;
    bridgeReady = false;
    csWarn(`Contexto da extensao invalidado; ignorando mensagens desta instancia. Motivo: ${err?.message || String(err)}`);
    for (const [id, pending] of pendingFromBg.entries()) {
      pendingFromBg.delete(id);
      pending.reject(new Error("Extension context invalidated"));
    }
  }

  async function safeSendRuntimeMessage(message) {
    if (!runtimeAvailable) return;
    try {
      return await chrome.runtime.sendMessage(message);
    } catch (err) {
      if (isExtensionContextInvalidated(err)) {
        invalidateRuntime(err);
        return;
      }
      throw err;
    }
  }

  csLog("content.js carregado");

  window.addEventListener("message", (event) => {
    if (!isActiveInstance() || !runtimeAvailable) return;
    if (event.source !== window) return;

    const msg = event.data;
    if (!msg) return;

    try {
      const { channel } = msg;

      if (channel === "cf-from-bridge" && msg.type === "bridge-ready") {
        bridgeReady = true;
        csLog("Bridge pronto. Notificando background...");
        safeSendRuntimeMessage({ action: "bridgeReady" }).catch((err) => {
          csWarn(`Falha ao notificar bridgeReady para background: ${err.message}`);
        });
        return;
      }

      if (channel === "cf-from-bridge" && msg.id) {
        csLog(`Resposta do bridge: id=${msg.id}, success=${msg.ok}`);
        const pending = pendingFromBg.get(msg.id);
        if (pending) {
          pendingFromBg.delete(msg.id);
          pending.resolve(msg);
        }
        return;
      }

      if (channel === "cf-from-bridge") {
        safeSendRuntimeMessage({ action: "bridgeEvent", data: msg }).catch((err) => {
          csWarn(`Falha ao encaminhar evento espontaneo para background: ${err.message}`);
        });
      }
    } catch (err) {
      if (isExtensionContextInvalidated(err)) {
        invalidateRuntime(err);
        return;
      }
      csError(`Erro ao processar mensagem do page_bridge: ${err.message}`, err);
    }
  });

  chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
    if (!isActiveInstance() || !runtimeAvailable) return false;
    if (msg.action !== "toPageBridge") return false;

    try {
      const { id, type, payload } = msg;
      csLog(`Comando do background para bridge: type=${type}, id=${id}`);

      const message = { channel: "cf-to-bridge", ...payload, id, type };
      window.postMessage(message, "*");

      let resolved = false;
      const timeoutMs = 5 * 60 * 1000;
      const timeout = setTimeout(() => {
        if (resolved) return;
        resolved = true;
        pendingFromBg.delete(id);
        csWarn(`Timeout aguardando page_bridge (${timeoutMs}ms): type=${type}, id=${id}`);
        sendResponse({
          id,
          type,
          ok: false,
          error: `Timeout aguardando page_bridge (${Math.round(timeoutMs / 1000)}s)`,
        });
      }, timeoutMs);

      pendingFromBg.set(id, {
        resolve: (data) => {
          if (resolved) return;
          resolved = true;
          clearTimeout(timeout);
          csLog(`Resposta recebida: type=${type}, id=${id}`);
          sendResponse(data);
        },
        reject: (err) => {
          if (resolved) return;
          resolved = true;
          clearTimeout(timeout);
          csError(`Rejeicao recebida: type=${type}, id=${id}, error=${String(err)}`);
          sendResponse({ id, type, ok: false, error: String(err) });
        },
      });

      return true;
    } catch (err) {
      if (isExtensionContextInvalidated(err)) {
        invalidateRuntime(err);
        sendResponse({ ok: false, error: "Extension context invalidated" });
        return false;
      }
      csError(`Erro ao processar comando do background: ${err.message}`, err);
      sendResponse({ ok: false, error: `Erro interno content.js: ${err.message}` });
      return true;
    }
  });
})();
