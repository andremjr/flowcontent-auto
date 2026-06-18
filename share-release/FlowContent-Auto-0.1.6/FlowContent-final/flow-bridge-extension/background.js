// background.js - Service Worker
// WebSocket client para o app Tauri (ws://127.0.0.1:9999/10000/10001)
// Roteia comandos do app -> content.js -> page_bridge.js (MAIN world)

const BRIDGE_TOKEN = "390d22e5-da0d-4533-bcea-8d258f76bf0b";
const WS_PORTS = [9999, 10000, 10001];
const HEARTBEAT_INTERVAL = 25_000;
const RECONNECT_DELAY = 3_000;
const COMMAND_TIMEOUT_MS = 5 * 60 * 1000;
const MAX_COMPLETED_CACHE = 200;

let currentPortIndex = 0;
let ws = null;
let wsReady = false;
let wsAuthenticated = false;
let heartbeatTimer = null;
let reconnectTimer = null;
let flowTabId = null;
let connectCount = 0;

const pendingRequests = new Map(); // reqId -> { resolve, reject, timeout }
const activeCommands = new Map(); // commandId -> { type, localProjectId, startedAt }
const completedCommandIds = [];
const completedCommandSet = new Set();

function bgLog(...args) {
  const ts = new Date().toISOString();
  console.log(`[BG][${ts}]`, ...args);
}

function rememberCompletedCommand(commandId) {
  if (!commandId || completedCommandSet.has(commandId)) return;
  completedCommandIds.push(commandId);
  completedCommandSet.add(commandId);
  while (completedCommandIds.length > MAX_COMPLETED_CACHE) {
    const oldest = completedCommandIds.shift();
    if (oldest) completedCommandSet.delete(oldest);
  }
}

function clearPendingRequest(id, err) {
  const pending = pendingRequests.get(id);
  if (!pending) return;
  clearTimeout(pending.timeout);
  pendingRequests.delete(id);
  if (err) {
    pending.reject(err);
  }
}

function isMissingContentScriptError(err) {
  const message = err?.message || String(err);
  return message.includes("Receiving end does not exist")
    || message.includes("Could not establish connection");
}

async function findFlowTab() {
  const tabs = await chrome.tabs.query({ url: "https://labs.google/fx/*" });
  const tab = tabs.find((candidate) => candidate.url && candidate.url.includes("/fx/"));
  if (tab) {
    if (flowTabId !== tab.id) {
      bgLog(`Aba Google Flow encontrada: tabId=${tab.id}, url=${tab.url}`);
    }
    flowTabId = tab.id;
  }
  return tab || null;
}

async function injectBridgeScripts(tabId) {
  bgLog(`Content relay ausente na aba ${tabId}; injetando scripts...`);
  try {
    await chrome.scripting.executeScript({
      target: { tabId },
      files: ["content.js"],
    });
    await chrome.scripting.executeScript({
      target: { tabId },
      world: "MAIN",
      files: ["page_bridge.js"],
    });
    bgLog(`Scripts da extensao injetados na aba ${tabId}`);
  } catch (err) {
    console.error(`[BG] Erro ao injetar scripts na aba ${tabId}: ${err.message}`);
  }
}

async function sendToContentScript(tabId, message) {
  try {
    return await chrome.tabs.sendMessage(tabId, message);
  } catch (err) {
    if (!isMissingContentScriptError(err)) throw err;
    await injectBridgeScripts(tabId);
    return chrome.tabs.sendMessage(tabId, message);
  }
}

async function callPageBridge(id, type, payload = {}) {
  const tab = await findFlowTab();
  if (!tab) {
    throw new Error("Nenhuma aba do Google Flow encontrada. Abra o Flow e faca login.");
  }

  bgLog(`-> Enviando para content.js: type=${type}, id=${id}, tabId=${tab.id}`);

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingRequests.delete(id);
      reject(new Error(`Timeout aguardando resposta da extensao (${Math.round(COMMAND_TIMEOUT_MS / 1000)}s)`));
    }, COMMAND_TIMEOUT_MS);

    pendingRequests.set(id, {
      resolve: (data) => {
        clearTimeout(timeout);
        pendingRequests.delete(id);
        resolve(data);
      },
      reject: (err) => {
        clearTimeout(timeout);
        pendingRequests.delete(id);
        reject(err);
      },
      timeout,
    });

    sendToContentScript(tab.id, { action: "toPageBridge", id, type, payload })
      .then((response) => {
        if (response) {
          clearPendingRequest(id);
          bgLog(`<- Resposta do content.js: type=${type}, id=${id}, ok=${response.ok}`);
          resolve(response);
        }
      })
      .catch((err) => {
        clearPendingRequest(id, new Error(`content.js nao respondeu: ${err.message}`));
      });
  });
}

function sendToApp(obj) {
  if (!ws || !wsReady) return;
  try {
    ws.send(JSON.stringify(obj));
  } catch (err) {
    console.error(`[BG] Falha ao enviar para app: ${err.message}`);
  }
}

function sendCommandAck(command) {
  sendToApp({
    type: "FLOWCONTENT_COMMAND_ACK",
    payload: {
      id: command.id,
      type: command.type,
      localProjectId: command.localProjectId || null,
      ackedAt: Date.now(),
    },
  });
}

function replayActiveCommandAcks() {
  for (const [id, command] of activeCommands.entries()) {
    bgLog(`Reenviando ACK de comando ativo apos reconnect: id=${id}, type=${command.type}`);
    sendCommandAck({ id, type: command.type, localProjectId: command.localProjectId });
  }
}

async function ensureProjectContext(command) {
  if (!["GENERATE_IMAGE", "GENERATE_VIDEO", "GENERATE_VIDEO_FROM_IMAGE", "ANIMATE_IMAGE"].includes(command.type) || !command.projectId) {
    return;
  }

  const tab = await findFlowTab();
  if (!tab) {
    throw new Error("Nenhuma aba do Google Flow encontrada. Abra o Flow e faca login.");
  }

  const targetPath = `/project/${command.projectId}`;
  if (tab.url && tab.url.includes(targetPath)) {
    bgLog(`Ja na pagina do projeto. Encaminhando ${command.type}.`);
    return;
  }

  const targetUrl = `https://labs.google/fx/pt/tools/flow/project/${command.projectId}`;
  bgLog(`Navegando para: ${targetUrl}`);
  await chrome.tabs.update(tab.id, { url: targetUrl, active: true });

  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      chrome.tabs.onUpdated.removeListener(listener);
      reject(new Error("Timeout aguardando pagina do projeto carregar."));
    }, 25_000);

    function listener(tabId, changeInfo) {
      if (tabId === tab.id && changeInfo.status === "complete") {
        clearTimeout(timeout);
        chrome.tabs.onUpdated.removeListener(listener);
        resolve();
      }
    }

    chrome.tabs.onUpdated.addListener(listener);
  });

  await new Promise((resolve) => setTimeout(resolve, 3_000));
  await injectBridgeScripts(tab.id);
  await new Promise((resolve) => setTimeout(resolve, 2_000));
}

async function handleAppRequest(req) {
  if (!req?.command) return;

  const command = req.command;
  const { id, type } = command;

  if (!id || !type) {
    bgLog("Comando recebido sem id ou type; ignorando.");
    return;
  }

  if (completedCommandSet.has(id)) {
    bgLog(`Ignorando redelivery de comando ja concluido: id=${id}, type=${type}`);
    return;
  }

  if (activeCommands.has(id)) {
    bgLog(`Ignorando redelivery de comando ja ativo: id=${id}, type=${type}`);
    sendCommandAck(command);
    return;
  }

  bgLog(`Comando recebido do app: id=${id}, type=${type}`);
  activeCommands.set(id, {
    type,
    localProjectId: command.localProjectId || null,
    startedAt: Date.now(),
  });
  sendCommandAck(command);

  if (type === "NAVIGATE_TO_PROJECT") {
    try {
      await ensureProjectContext(command);
      sendToApp({
        type: "FLOWCONTENT_COMMAND_RESULT",
        payload: {
          id,
          type,
          ok: true,
          navigated: true,
          localProjectId: command.localProjectId || null,
        },
      });
      activeCommands.delete(id);
      rememberCompletedCommand(id);
    } catch (err) {
      sendToApp({
        type: "FLOWCONTENT_COMMAND_RESULT",
        payload: {
          id,
          type,
          ok: false,
          localProjectId: command.localProjectId || null,
          error: err.message || String(err),
        },
      });
      activeCommands.delete(id);
      rememberCompletedCommand(id);
    }
    return;
  }

  try {
    await ensureProjectContext(command);
    const result = await callPageBridge(id, type, command);
    sendToApp({
      type: "FLOWCONTENT_COMMAND_RESULT",
      payload: result,
    });
  } catch (err) {
    sendToApp({
      type: "FLOWCONTENT_COMMAND_RESULT",
      payload: {
        id,
        type,
        ok: false,
        localProjectId: command.localProjectId || null,
        error: err.message || String(err),
      },
    });
  } finally {
    activeCommands.delete(id);
    rememberCompletedCommand(id);
  }
}

chrome.runtime.onMessage.addListener((msg, sender) => {
  if (msg.action === "bridgeReady") {
    bgLog(`page_bridge.js pronto na aba ${sender.tab?.id}`);
    flowTabId = sender.tab?.id || flowTabId;
    return;
  }

  if (msg.action !== "bridgeEvent") return;
  const eventData = msg.data;
  if (!eventData) return;

  if (eventData.type === "FLOWCONTENT_HEARTBEAT") {
    sendToApp({ type: "FLOWCONTENT_HEARTBEAT", payload: eventData.payload });
    return;
  }

  if (eventData.type === "FLOWCONTENT_PROGRESS") {
    sendToApp({ type: "FLOWCONTENT_PROGRESS", payload: eventData.payload });
    return;
  }

  if (eventData.type === "FLOWCONTENT_LOG") {
    sendToApp({ type: "FLOWCONTENT_LOG", payload: eventData.payload });
    return;
  }

  if (eventData.type === "FLOWCONTENT_COMMAND_RESULT") {
    const commandId = eventData.payload?.id;
    if (commandId) {
      activeCommands.delete(commandId);
      rememberCompletedCommand(commandId);
    }
    sendToApp({ type: "FLOWCONTENT_COMMAND_RESULT", payload: eventData.payload });
  }
});

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, RECONNECT_DELAY);
}

function startHeartbeat() {
  stopHeartbeat();
  heartbeatTimer = setInterval(() => {
    if (ws && ws.readyState === WebSocket.OPEN && wsAuthenticated) {
      ws.send(JSON.stringify({ type: "heartbeat" }));
    } else {
      stopHeartbeat();
    }
  }, HEARTBEAT_INTERVAL);
}

function stopHeartbeat() {
  if (!heartbeatTimer) return;
  clearInterval(heartbeatTimer);
  heartbeatTimer = null;
}

function sendHello() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({
    type: "FLOWCONTENT_WS_HELLO",
    payload: {
      token: BRIDGE_TOKEN,
      extensionVersion: chrome.runtime.getManifest().version,
      activeCommandIds: [...activeCommands.keys()],
      observedAt: Date.now(),
    },
  }));
}

function connect() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  if (ws && (ws.readyState === WebSocket.CONNECTING || ws.readyState === WebSocket.OPEN)) {
    return;
  }

  const port = WS_PORTS[currentPortIndex % WS_PORTS.length];
  const wsUrl = `ws://127.0.0.1:${port}`;
  connectCount += 1;
  wsReady = false;
  wsAuthenticated = false;
  bgLog(`Tentando conexao WS #${connectCount} para ${wsUrl}...`);

  try {
    ws = new WebSocket(wsUrl);
  } catch (err) {
    console.error(`[BG] Falha ao criar WebSocket na porta ${port}: ${err.message}`);
    currentPortIndex += 1;
    scheduleReconnect();
    return;
  }

  ws.onopen = () => {
    wsAuthenticated = true;
    wsReady = true;
    bgLog(`WebSocket aberto na porta ${port}.`);
    sendHello();
    replayActiveCommandAcks();
    sendToApp({
      type: "FLOWCONTENT_HEARTBEAT",
      payload: { pageDetected: false },
    });
    startHeartbeat();
  };

  ws.onmessage = (event) => {
    try {
      const req = JSON.parse(event.data);
      if (req.type === "heartbeat") return;

      if (req.type === "FLOWCONTENT_WS_AUTH_OK" || req.type === "FLOWCONTENT_WS_AUTH_ERROR") return;

      handleAppRequest(req);
    } catch (err) {
      console.warn(`[BG] Mensagem invalida recebida: ${err.message}`);
    }
  };

  ws.onerror = () => {
    wsReady = false;
    wsAuthenticated = false;
  };

  ws.onclose = (event) => {
    bgLog(`WebSocket desconectado (code=${event.code}).`);
    wsReady = false;
    wsAuthenticated = false;
    stopHeartbeat();
    if (event.code !== 1000 && event.code !== 4001) {
      currentPortIndex += 1;
    }
    scheduleReconnect();
  };
}

chrome.alarms.create("keepalive", { periodInMinutes: 0.4 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "keepalive") {
    if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
      connect();
    }
  }
});

chrome.runtime.onStartup.addListener(() => {
  connect();
});

chrome.runtime.onInstalled.addListener(async () => {
  chrome.alarms.create("keepalive", { periodInMinutes: 0.4 });
  const tabs = await chrome.tabs.query({ url: "https://labs.google/fx/*" });
  await Promise.allSettled(
    tabs.filter((tab) => tab.id).map((tab) => injectBridgeScripts(tab.id)),
  );
  connect();
});

bgLog("Service Worker carregado. Conectando...");
connect();
