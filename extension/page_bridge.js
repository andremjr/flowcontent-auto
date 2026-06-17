// page_bridge.js — Injetado no MAIN world em labs.google/fx/*
// Roda no mesmo contexto JS da página do Google Flow.

(function () {
  if (window.__cfBridgeLoaded) return;
  window.__cfBridgeLoaded = true;

  function pbLog(...args) {
    const ts = new Date().toISOString();
    const msg = args.map(arg => typeof arg === 'object' ? JSON.stringify(arg) : String(arg)).join(' ');
    console.log(`[PB][${ts}]`, msg);
    try {
      window.postMessage({
        channel: 'cf-from-bridge',
        type: 'FLOWCONTENT_LOG',
        payload: { timestamp: ts, message: msg }
      }, '*');
    } catch (e) {}
  }

  function pbError(...args) {
    const ts = new Date().toISOString();
    const msg = args.map(arg => typeof arg === 'object' ? JSON.stringify(arg) : String(arg)).join(' ');
    console.error(`[PB][${ts}]`, msg);
    try {
      window.postMessage({
        channel: 'cf-from-bridge',
        type: 'FLOWCONTENT_LOG',
        payload: { timestamp: ts, message: `[ERROR] ${msg}` }
      }, '*');
    } catch (e) {}
  }

  pbLog('🏁 page_bridge.js carregado com sucesso.');

  // ── Global Credentials Interceptor ──────────────────────────────────────────
  let latestAuthHeader = sessionStorage.getItem("__cf_latest_auth_token") || null;

  // Intercept window.fetch to capture Authorization header
  const originalFetch = window.fetch;
  window.fetch = async function (input, init) {
    let url = "";
    if (typeof input === "string") {
      url = input;
    } else if (input && typeof input === "object") {
      url = input.url || "";
    }

    if (url.includes("aisandbox-pa.googleapis.com") || url.includes("googleapis.com")) {
      let authHeader = null;

      if (init && init.headers) {
        if (init.headers instanceof Headers) {
          authHeader = init.headers.get("authorization");
        } else if (Array.isArray(init.headers)) {
          const found = init.headers.find(h => h[0]?.toLowerCase() === "authorization");
          authHeader = found ? found[1] : null;
        } else if (typeof init.headers === "object") {
          const key = Object.keys(init.headers).find(k => k.toLowerCase() === "authorization");
          if (key) {
            authHeader = init.headers[key];
          }
        }
      }

      if (!authHeader && input && typeof input === "object" && input.headers) {
        if (input.headers instanceof Headers) {
          authHeader = input.headers.get("authorization");
        }
      }

      if (authHeader) {
        if (latestAuthHeader !== authHeader) {
          latestAuthHeader = authHeader;
          sessionStorage.setItem("__cf_latest_auth_token", authHeader);
          pbLog("Authorization header do Flow atualizado na sessao atual.");
        }
      }
    }

    return originalFetch.apply(this, arguments);
  };

  // Intercept XMLHttpRequest to capture Authorization header
  const originalOpen = XMLHttpRequest.prototype.open;
  const originalSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;

  XMLHttpRequest.prototype.open = function (method, url) {
    this._url = url;
    return originalOpen.apply(this, arguments);
  };

  XMLHttpRequest.prototype.setRequestHeader = function (header, value) {
    if (header.toLowerCase() === "authorization") {
      if (this._url && (this._url.includes("aisandbox-pa.googleapis.com") || this._url.includes("googleapis.com"))) {
        if (latestAuthHeader !== value) {
          latestAuthHeader = value;
          sessionStorage.setItem("__cf_latest_auth_token", value);
          pbLog("Authorization header XHR do Flow atualizado na sessao atual.");
        }
      }
    }
    return originalSetRequestHeader.apply(this, arguments);
  };

  // ── Helper: extrair ID do projeto da URL ────────────────────────────────────
  function projectIdFromUrl() {
    return location.pathname.match(/\/flow\/project\/([a-f0-9-]{20,})/i)?.[1] ?? null;
  }

  const wait = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
  const normalize = (value) => String(value || "").normalize("NFD").replace(/[\u0300-\u036f]/g, "").trim().toLowerCase();
  
  const visible = (element) => {
    if (!(element instanceof HTMLElement)) return false;
    const style = getComputedStyle(element);
    const bounds = element.getBoundingClientRect();
    return style.display !== "none" && style.visibility !== "hidden" && bounds.width > 0 && bounds.height > 0;
  };

  function findProjectId(value, parentKey = "") {
    if (typeof value === "string") {
      const match = value.match(/[a-f0-9]{8}-[a-f0-9]{4}-[1-5][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}/i);
      return match && /(^|project)id$/i.test(parentKey) ? match[0] : null;
    }
    if (Array.isArray(value)) {
      for (const item of value) {
        const projectId = findProjectId(item, parentKey);
        if (projectId) return projectId;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    for (const [key, item] of Object.entries(value)) {
      const projectId = findProjectId(item, key);
      if (projectId) return projectId;
    }
    return null;
  }

  function buttonByText(candidates) {
    const expected = candidates.map(normalize);
    return [...document.querySelectorAll("button, [role='button']")]
      .find((element) => visible(element) && expected.some((text) => normalize(element.textContent).includes(text)));
  }

  function setComposerValue(composer, value) {
    if (composer instanceof HTMLTextAreaElement || composer instanceof HTMLInputElement) {
      const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(composer), "value")?.set;
      setter?.call(composer, value);
    } else {
      composer.textContent = value;
    }
    composer.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
    composer.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // ── Create Project via Google Flow trpc endpoint ───────────────────────────
  async function createProject(command) {
    pbLog("Iniciando createProject para o comando:", command);
    const response = await fetch("https://labs.google/fx/api/trpc/project.createProject", {
      method: "POST",
      credentials: "include",
      headers: {
        "Accept": "*/*",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        json: {
          projectTitle: command.title,
          toolName: "PINHOLE",
        },
      }),
    });
    
    pbLog("Resposta do fetch de createProject recebida, status:", response.status);
    const payload = await response.json().catch(() => null);
    pbLog("Payload retornado do createProject:", payload);
    
    if (!response.ok) {
      const message = payload?.error?.json?.message || payload?.error?.message || `HTTP ${response.status}`;
      throw new Error(`O Flow recusou a criacao do projeto: ${message}`);
    }
    
    const projectId = findProjectId(payload);
    if (!projectId) throw new Error("O Flow criou o projeto, mas nao retornou um ID reconhecivel.");
    
    pbLog("Projeto criado no Flow com ID:", projectId);
    return {
      projectId,
      localProjectId: command.localProjectId,
      openUrl: `https://labs.google/fx/pt/tools/flow/project/${projectId}`,
    };
  }

  // ── Helper: gerar UUID v4 ──────────────────────────────────────────────────
  function uuidv4() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0, v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  }

  // ── Helper: Executar funcao com retentativas ───────────────────────────────
  async function runWithRetry(fn, label, retries = 3) {
    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        return await fn();
      } catch (err) {
        pbError(`Falha em [${label}] na tentativa ${attempt}/${retries}: ${err.message}`);
        if (attempt === retries) throw err;
        await wait(2000 * attempt); // Delay exponencial
      }
    }
  }

  // ── Helper: Obter token reCAPTCHA Enterprise ──────────────────────────────
  async function getRecaptchaToken(action) {
    const siteKey = "6LdsFiUsAAAAAIjVDZcuLhaHiDn5nnHVXVRQGeMV";
    if (window.grecaptcha && window.grecaptcha.enterprise) {
      pbLog(`Solicitando token reCAPTCHA para ${action}...`);
      return await window.grecaptcha.enterprise.execute(siteKey, { action });
    }
    throw new Error("reCAPTCHA Enterprise nao esta inicializado no site.");
  }

  // ── Direct API: Gerar Imagem ────────────────────────────────────────────────
  async function generateImageDirect(projectId, prompt, sessionId, batchId, imageModel, imageAspectRatio) {
    const recaptchaToken = await getRecaptchaToken("IMAGE_GENERATION");
    const url = `https://aisandbox-pa.googleapis.com/v1/projects/${projectId}/flowMedia:batchGenerateImages`;
    
    const clientContext = {
      recaptchaContext: {
        token: recaptchaToken,
        applicationType: "RECAPTCHA_APPLICATION_TYPE_WEB"
      },
      projectId: projectId,
      tool: "PINHOLE",
      sessionId: sessionId
    };

    const body = {
      clientContext,
      mediaGenerationContext: { batchId },
      useNewMedia: true,
      requests: [
        {
          clientContext,
          imageModelName: imageModel || "GEM_PIX_2",
          imageAspectRatio: imageAspectRatio || "IMAGE_ASPECT_RATIO_LANDSCAPE",
          structuredPrompt: {
            parts: [{ text: prompt }]
          },
          seed: Math.floor(Math.random() * 1000000),
          imageInputs: []
        }
      ]
    };

    pbLog(`Chamando batchGenerateImages para o prompt: "${prompt}"`);
    const headers = {
      "Accept": "application/json",
      "Content-Type": "application/json"
    };
    if (latestAuthHeader) {
      headers["Authorization"] = latestAuthHeader;
    } else {
      pbLog("⚠️ Token de autorização não capturado ainda. Enviando sem token.");
    }
    const response = await fetch(url, {
      method: "POST",
      credentials: "include",
      headers,
      body: JSON.stringify(body)
    });

    if (!response.ok) {
      const errText = await response.text().catch(() => "");
      throw new Error(`Erro API Imagem (${response.status}): ${errText}`);
    }

    const payload = await response.json();
    const mediaItem = payload?.media?.[0];
    if (!mediaItem || !mediaItem.name) {
      throw new Error(`Resposta de imagem invalida ou vazia.`);
    }

    const mediaId = mediaItem.name;
    const fifeUrl = mediaItem.image?.generatedImage?.fifeUrl;
    if (!fifeUrl) {
      throw new Error(`URL de imagem (fifeUrl) ausente na resposta.`);
    }

    return {
      mediaId,
      url: fifeUrl,
      workflowId: mediaItem.workflowId || mediaItem.image?.generatedImage?.workflowId || payload?.workflows?.[0]?.name || null,
      batchId: payload?.workflows?.[0]?.metadata?.batchId || batchId,
    };
  }

  // ── Direct API: Iniciar Video Texto (Text-to-Video) ─────────────────────────
  async function generateVideoTextDirect(projectId, prompt, sessionId, batchId, videoModel, videoAspectRatio) {
    const recaptchaToken = await getRecaptchaToken("VIDEO_GENERATION");
    const url = `https://aisandbox-pa.googleapis.com/v1/video:batchAsyncGenerateVideoText`;
    
    const clientContext = {
      projectId: projectId,
      tool: "PINHOLE",
      userPaygateTier: "PAYGATE_TIER_TWO",
      sessionId: sessionId,
      recaptchaContext: {
        token: recaptchaToken,
        applicationType: "RECAPTCHA_APPLICATION_TYPE_WEB"
      }
    };

    const body = {
      mediaGenerationContext: {
        batchId,
        audioFailurePreference: "BLOCK_SILENCED_VIDEOS"
      },
      clientContext,
      requests: [
        {
          aspectRatio: videoAspectRatio || "VIDEO_ASPECT_RATIO_LANDSCAPE",
          textInput: {
            structuredPrompt: {
              parts: [{ text: prompt }]
            }
          },
          videoModelKey: videoModel || "veo_3_1_t2v_lite_low_priority",
          seed: Math.floor(Math.random() * 100000),
          metadata: {}
        }
      ],
      useV2ModelConfig: true
    };

    pbLog(`Chamando batchAsyncGenerateVideoText para o prompt: "${prompt}"`);
    const headers = {
      "Accept": "application/json",
      "Content-Type": "application/json"
    };
    if (latestAuthHeader) {
      headers["Authorization"] = latestAuthHeader;
    } else {
      pbLog("⚠️ Token de autorização não capturado ainda. Enviando sem token.");
    }
    const response = await fetch(url, {
      method: "POST",
      credentials: "include",
      headers,
      body: JSON.stringify(body)
    });

    if (!response.ok) {
      const errText = await response.text().catch(() => "");
      throw new Error(`Erro API VideoTexto (${response.status}): ${errText}`);
    }

    const payload = await response.json();
    const mediaItem = payload?.media?.[0];
    if (!mediaItem || !mediaItem.name) {
      throw new Error(`Resposta de video texto invalida ou vazia.`);
    }

    return {
      mediaId: mediaItem.name,
      workflowId: mediaItem.workflowId || null,
      operationId: mediaItem.video?.operation?.name || null,
      batchId,
    };
  }

  // ── Direct API: Iniciar Video Imagem (Image-to-Video / Animar) ──────────────
  async function generateVideoStartImageDirect(projectId, prompt, imageMediaId, sessionId, batchId, videoModel, videoAspectRatio) {
    const recaptchaToken = await getRecaptchaToken("VIDEO_GENERATION");
    const url = `https://aisandbox-pa.googleapis.com/v1/video:batchAsyncGenerateVideoStartImage`;
    
    const clientContext = {
      projectId: projectId,
      tool: "PINHOLE",
      userPaygateTier: "PAYGATE_TIER_TWO",
      sessionId: sessionId,
      recaptchaContext: {
        token: recaptchaToken,
        applicationType: "RECAPTCHA_APPLICATION_TYPE_WEB"
      }
    };

    const body = {
      mediaGenerationContext: {
        batchId,
        audioFailurePreference: "BLOCK_SILENCED_VIDEOS"
      },
      clientContext,
      requests: [
        {
          aspectRatio: videoAspectRatio || "VIDEO_ASPECT_RATIO_LANDSCAPE",
          textInput: {
            structuredPrompt: {
              parts: [{ text: prompt }]
            }
          },
          videoModelKey: videoModel || "veo_3_1_i2v_lite_low_priority",
          seed: Math.floor(Math.random() * 100000),
          metadata: {},
          startImage: {
            mediaId: imageMediaId,
            cropCoordinates: {
              top: 0,
              left: 0,
              bottom: 1,
              right: 1
            }
          }
        }
      ],
      useV2ModelConfig: true
    };

    pbLog(`Chamando batchAsyncGenerateVideoStartImage para prompt: "${prompt}" com imagem: ${imageMediaId}`);
    const headers = {
      "Accept": "application/json",
      "Content-Type": "application/json"
    };
    if (latestAuthHeader) {
      headers["Authorization"] = latestAuthHeader;
    } else {
      pbLog("⚠️ Token de autorização não capturado ainda. Enviando sem token.");
    }
    const response = await fetch(url, {
      method: "POST",
      credentials: "include",
      headers,
      body: JSON.stringify(body)
    });

    if (!response.ok) {
      const errText = await response.text().catch(() => "");
      throw new Error(`Erro API VideoImagem (${response.status}): ${errText}`);
    }

    const payload = await response.json();
    const mediaItem = payload?.media?.[0];
    if (!mediaItem || !mediaItem.name) {
      throw new Error(`Resposta de video imagem invalida ou vazia.`);
    }

    return {
      mediaId: mediaItem.name,
      workflowId: mediaItem.workflowId || null,
      operationId: mediaItem.video?.operation?.name || null,
      batchId,
    };
  }

  // ── Direct API: Polling de Status do Video ──────────────────────────────────
  function emitGenerationProgress(command, status, details = {}) {
    window.postMessage({
      channel: 'cf-from-bridge',
      type: 'FLOWCONTENT_PROGRESS',
      payload: {
        status,
        localProjectId: command.localProjectId,
        projectId: command.projectId,
        sourceOrder: command.sourceOrder,
        prompt: command.prompt,
        ...details,
      }
    }, '*');
  }

  async function pollVideoStatus(projectId, videoMediaId, command, initial = {}) {
    const url = `https://aisandbox-pa.googleapis.com/v1/video:batchCheckAsyncVideoGenerationStatus`;
    const body = {
      media: [
        {
          name: videoMediaId,
          projectId: projectId
        }
      ]
    };

    const maxAttempts = 150; // Limite de aprox 7.5 minutos
    let lastStatus = null;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      pbLog(`Consultando status de ${videoMediaId} (Tentativa ${attempt}/${maxAttempts})...`);
      const headers = {
        "Accept": "application/json",
        "Content-Type": "application/json"
      };
      if (latestAuthHeader) {
        headers["Authorization"] = latestAuthHeader;
      }
      const response = await fetch(url, {
        method: "POST",
        credentials: "include",
        headers,
        body: JSON.stringify(body)
      });

      if (!response.ok) {
        const errText = await response.text().catch(() => "");
        pbError(`Erro ao consultar status (${response.status}): ${errText}. Retentando...`);
      } else {
        const payload = await response.json();
        const mediaItem = payload?.media?.[0];
        const status = mediaItem?.mediaMetadata?.mediaStatus?.mediaGenerationStatus;
        
        pbLog(`Status recebido para ${videoMediaId}: ${status}`);

        if (status && status !== lastStatus) {
          lastStatus = status;
          emitGenerationProgress(command, 'VIDEO_STATUS', {
            mediaId: videoMediaId,
            imageMediaId: initial.imageMediaId || null,
            workflowId: mediaItem?.workflowId || initial.workflowId || null,
            batchId: initial.batchId || null,
            operationId: mediaItem?.video?.operation?.name || initial.operationId || null,
            remoteStatus: status,
            remainingCredits: payload?.remainingCredits ?? null,
          });
        }
        
        if (status === "MEDIA_GENERATION_STATUS_SUCCESSFUL") {
          return {
            remoteStatus: status,
            workflowId: mediaItem?.workflowId || null,
            operationId: mediaItem?.video?.operation?.name || null,
            remainingCredits: payload?.remainingCredits ?? null,
          };
        }
        if (status === "MEDIA_GENERATION_STATUS_FAILED") {
          throw new Error(`A geracao do video falhou no backend do Flow.`);
        }
      }

      await wait(3000);
    }

    throw new Error(`Timeout aguardando a geracao do video.`);
  }

  // ── Direct API: Obter URL Redirecionada Final ───────────────────────────────
  async function getRedirectedUrl(mediaId, mediaUrlType = null) {
    pbLog(`Obtendo URL CDN final redirecionada para mediaId: ${mediaId}`);
    const suffix = mediaUrlType ? `&mediaUrlType=${mediaUrlType}` : '';
    const res = await fetch(`https://labs.google/fx/api/trpc/media.getMediaUrlRedirect?name=${mediaId}${suffix}`);
    if (!res.ok) {
      throw new Error(`Erro ao obter redirecionamento (${res.status}) para ${mediaId}`);
    }
    pbLog(`URL CDN final: ${res.url}`);
    return res.url;
  }

  // ── Command: GENERATE_IMAGE ───────────────────────────────────────────────────
  // Single image generation. Captures fresh recaptcha + cookies from browser.
  async function handleGenerateImage(command) {
    const { projectId, prompt, sourceOrder, imageModel, imageAspectRatio } = command;
    const sessionId = ';' + Date.now();
    const batchId = uuidv4();
    
    pbLog(`[GENERATE_IMAGE] slot=${sourceOrder} prompt="${prompt}" model=${imageModel}`);
    
    const result = await generateImageDirect(
      projectId, prompt, sessionId, batchId,
      imageModel || 'GEM_PIX_2',
      imageAspectRatio || 'IMAGE_ASPECT_RATIO_LANDSCAPE'
    );
    
    return {
      sourceOrder,
      prompt,
      mediaId: result.mediaId,
      url: result.url,
      assetType: 'png',
      workflowId: result.workflowId,
      batchId: result.batchId,
      remoteStatus: 'REMOTE_SUCCESSFUL'
    };
  }

  // ── Command: GENERATE_VIDEO ──────────────────────────────────────────────────
  // Single text-to-video. Captures fresh recaptcha + cookies for start + polls.
  async function handleGenerateVideo(command) {
    const { projectId, prompt, sourceOrder, videoModel, videoAspectRatio } = command;
    const sessionId = ';' + Date.now();
    const batchId = uuidv4();

    pbLog(`[GENERATE_VIDEO] slot=${sourceOrder} prompt="${prompt}" model=${videoModel}`);

    const videoStart = await generateVideoTextDirect(
      projectId, prompt, sessionId, batchId,
      videoModel || 'veo_3_1_t2v_lite_low_priority',
      videoAspectRatio || 'VIDEO_ASPECT_RATIO_LANDSCAPE'
    );

    pbLog(`[GENERATE_VIDEO] slot=${sourceOrder} mediaId=${videoStart.mediaId}, aguardando conclusao...`);
    emitGenerationProgress(command, 'VIDEO_SCHEDULED', {
      mediaId: videoStart.mediaId,
      workflowId: videoStart.workflowId,
      batchId: videoStart.batchId,
      operationId: videoStart.operationId,
      remoteStatus: 'REMOTE_SCHEDULED',
    });
    const statusResult = await pollVideoStatus(projectId, videoStart.mediaId, command, videoStart);

    const assetUrl = await runWithRetry(async () => {
      return await getRedirectedUrl(videoStart.mediaId);
    }, `Obter URL Video slot ${sourceOrder}`);
    const thumbnailUrl = await runWithRetry(async () => {
      return await getRedirectedUrl(videoStart.mediaId, 'MEDIA_URL_TYPE_THUMBNAIL');
    }, `Obter thumbnail video slot ${sourceOrder}`).catch(() => null);

    return {
      sourceOrder,
      prompt,
      mediaId: videoStart.mediaId,
      url: assetUrl,
      assetType: 'mp4',
      workflowId: statusResult.workflowId || videoStart.workflowId,
      batchId: videoStart.batchId,
      operationId: statusResult.operationId || videoStart.operationId,
      thumbnailUrl,
      remoteStatus: statusResult.remoteStatus,
      remainingCredits: statusResult.remainingCredits,
    };
  }

  // ── Command: GENERATE_VIDEO_FROM_IMAGE ────────────────────────────────────────
  // Image → Animate. Two API calls, each with fresh recaptcha + cookies.
  async function handleGenerateVideoFromImage(command) {
    const { projectId, prompt, sourceOrder, imageModel, imageAspectRatio, videoModel, videoAspectRatio } = command;
    const sessionId = ';' + Date.now();

    pbLog(`[GENERATE_VIDEO_FROM_IMAGE] slot=${sourceOrder} prompt="${prompt}"`);

    // Step 1: Generate the base image (fresh auth)
    const imgResult = await generateImageDirect(
      projectId, prompt, sessionId, uuidv4(),
      imageModel || 'GEM_PIX_2',
      imageAspectRatio || 'IMAGE_ASPECT_RATIO_LANDSCAPE'
    );

    emitGenerationProgress(command, 'IMAGE_READY', {
      mediaId: imgResult.mediaId,
      url: imgResult.url,
      assetType: 'png',
      workflowId: imgResult.workflowId,
      batchId: imgResult.batchId,
      remoteStatus: 'REMOTE_IMAGE_READY',
    });

    pbLog(`[GENERATE_VIDEO_FROM_IMAGE] slot=${sourceOrder} imagem=${imgResult.mediaId}, animando...`);

    // Step 2: Animate the image (fresh auth)
    const videoStart = await generateVideoStartImageDirect(
      projectId, prompt, imgResult.mediaId, sessionId, uuidv4(),
      videoModel || 'veo_3_1_i2v_lite_low_priority',
      videoAspectRatio || 'VIDEO_ASPECT_RATIO_LANDSCAPE'
    );

    pbLog(`[GENERATE_VIDEO_FROM_IMAGE] slot=${sourceOrder} video=${videoStart.mediaId}, aguardando...`);
    emitGenerationProgress(command, 'VIDEO_SCHEDULED', {
      mediaId: videoStart.mediaId,
      imageMediaId: imgResult.mediaId,
      workflowId: videoStart.workflowId,
      batchId: videoStart.batchId,
      operationId: videoStart.operationId,
      remoteStatus: 'REMOTE_SCHEDULED',
    });
    const statusResult = await pollVideoStatus(projectId, videoStart.mediaId, command, {
      ...videoStart,
      imageMediaId: imgResult.mediaId,
    });

    const assetUrl = await runWithRetry(async () => {
      return await getRedirectedUrl(videoStart.mediaId);
    }, `Obter URL Animacao slot ${sourceOrder}`);
    const thumbnailUrl = await runWithRetry(async () => {
      return await getRedirectedUrl(videoStart.mediaId, 'MEDIA_URL_TYPE_THUMBNAIL');
    }, `Obter thumbnail animacao slot ${sourceOrder}`).catch(() => null);

    return {
      sourceOrder,
      prompt,
      mediaId: videoStart.mediaId,
      imageMediaId: imgResult.mediaId,
      url: assetUrl,
      assetType: 'mp4',
      workflowId: statusResult.workflowId || videoStart.workflowId,
      batchId: videoStart.batchId,
      operationId: statusResult.operationId || videoStart.operationId,
      thumbnailUrl,
      remoteStatus: statusResult.remoteStatus,
      remainingCredits: statusResult.remainingCredits,
    };
  }

  // ── Command: ANIMATE_IMAGE ────────────────────────────────────────────────
  // Animate an already generated image using its existing Flow media id.
  async function handleAnimateImage(command) {
    const { projectId, prompt, sourceOrder, imageMediaId, videoModel, videoAspectRatio, i2vModel } = command;
    if (!imageMediaId) {
      throw new Error(`Slot ${sourceOrder} não possui imageMediaId para animação.`);
    }
    const sessionId = ';' + Date.now();

    pbLog(`[ANIMATE_IMAGE] slot=${sourceOrder} image=${imageMediaId}`);

    const videoStart = await generateVideoStartImageDirect(
      projectId,
      prompt,
      imageMediaId,
      sessionId,
      uuidv4(),
      i2vModel || videoModel || 'veo_3_1_i2v_lite_low_priority',
      videoAspectRatio || 'VIDEO_ASPECT_RATIO_LANDSCAPE'
    );

    pbLog(`[ANIMATE_IMAGE] slot=${sourceOrder} video=${videoStart.mediaId}, aguardando...`);
    emitGenerationProgress(command, 'VIDEO_SCHEDULED', {
      mediaId: videoStart.mediaId,
      imageMediaId,
      workflowId: videoStart.workflowId,
      batchId: videoStart.batchId,
      operationId: videoStart.operationId,
      remoteStatus: 'REMOTE_SCHEDULED',
    });
    const statusResult = await pollVideoStatus(projectId, videoStart.mediaId, command, {
      ...videoStart,
      imageMediaId,
    });

    const assetUrl = await runWithRetry(async () => {
      return await getRedirectedUrl(videoStart.mediaId);
    }, `Obter URL Animacao Existente slot ${sourceOrder}`);
    const thumbnailUrl = await runWithRetry(async () => {
      return await getRedirectedUrl(videoStart.mediaId, 'MEDIA_URL_TYPE_THUMBNAIL');
    }, `Obter thumbnail animacao existente slot ${sourceOrder}`).catch(() => null);

    return {
      sourceOrder,
      prompt,
      mediaId: videoStart.mediaId,
      imageMediaId,
      url: assetUrl,
      assetType: 'mp4',
      workflowId: statusResult.workflowId || videoStart.workflowId,
      batchId: videoStart.batchId,
      operationId: statusResult.operationId || videoStart.operationId,
      thumbnailUrl,
      remoteStatus: statusResult.remoteStatus,
      remainingCredits: statusResult.remainingCredits,
    };
  }

  // ── Handlers de comando ─────────────────────────────────────────────────────
  const handlers = {
    'CREATE_PROJECT': createProject,
    'GENERATE_IMAGE': handleGenerateImage,
    'GENERATE_VIDEO': handleGenerateVideo,
    'GENERATE_VIDEO_FROM_IMAGE': handleGenerateVideoFromImage,
    'ANIMATE_IMAGE': handleAnimateImage,
  };

  // ── Message Listener (commands from content.js) ─────────────────────────────
  window.addEventListener('message', async (event) => {
    if (event.source !== window) return;
    const msg = event.data;
    if (!msg || msg.channel !== 'cf-to-bridge') return;

    const { id, type } = msg;
    pbLog(`⬇ Comando recebido: type=${type}, id=${id}`);

    const reply = (payload) => {
      pbLog(`⬆ Enviando resposta: type=${type}, id=${id}, ok=${payload.ok}`);
      window.postMessage({
        channel: 'cf-from-bridge',
        id,
        type,
        localProjectId: msg.localProjectId || null,
        projectId: msg.projectId || null,
        ...payload
      }, '*');
    };

    try {
      const handler = handlers[type];
      if (handler) {
        const result = await handler(msg);
        reply({ ok: true, ...result });
      } else {
        reply({ ok: false, error: `Tipo de comando desconhecido: ${type}` });
      }
    } catch (err) {
      pbError(`✖ Erro ao executar comando: ${err.message}`);
      reply({ ok: false, localProjectId: msg.localProjectId, error: err.message || String(err) });
    }
  });

  // ── Heartbeat Loop ──────────────────────────────────────────────────────────
  function emitHeartbeat() {
    window.postMessage({
      channel: 'cf-from-bridge',
      type: 'FLOWCONTENT_HEARTBEAT',
      payload: {
        version: 1,
        url: location.href,
        title: document.title,
        projectId: projectIdFromUrl(),
        pageDetected: true,
        observedAt: Date.now()
      }
    }, '*');
  }

  // Emitir heartbeat inicial e configurar intervalo a cada 3 segundos
  emitHeartbeat();
  setInterval(emitHeartbeat, 3000);
  window.addEventListener("hashchange", emitHeartbeat);
  window.addEventListener("popstate", emitHeartbeat);

  // Sinalizar que a ponte do MAIN world está pronta
  window.postMessage({ channel: 'cf-from-bridge', type: 'bridge-ready' }, '*');
})();
