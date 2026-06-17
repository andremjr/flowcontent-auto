import { useEffect, useMemo, useRef, useState } from "react";
import { recordDiagnostic } from "../lib/diagnostics";
import {
  authenticate,
  checkForUpdate,
  chooseAudio,
  clearAssemblyAiKeys,
  createProject,
  deleteProject,
  downloadProjectSrt,
  ensureFlowProjectLink,
  exportCapcutProject,
  getAssemblyAiStatus,
  getAuthStatus,
  getFlowBridgeStatus,
  getGenerationProgress,
  getProjectDetail,
  getUpdateStatus,
  importProjectPrompts,
  initializeWorkspace,
  installPendingUpdate,
  isDesktopApp,
  listProjects,
  lockApp,
  openFlowBrowser,
  pauseProjectGeneration,
  processProjectAudio,
  queueProjectAnimation,
  queueProjectGeneration,
  getSlotVideoPreviewDataUrl,
  readLocalVideoBlobPayload,
  readLocalImageDataUrl,
  reconcileProjectSlotAsset,
  retryFailedGenerations,
  saveAssemblyAiKeys,
  validateLicense,
} from "../lib/desktop";
import {
  canAnimateSlot,
  hasFinalLocalAsset,
  hasVisibleProgress,
  initialAssemblyStatus,
  initialBridgeStatus,
  messageFrom,
  normalizeGenerationSlot,
  projectPromptTotal,
  sameBridgeStatus,
  slotOrderFromFilename,
  splitPrompts,
} from "../lib/app-state";
import type {
  AssemblyAiStatus,
  DownloadedAsset,
  FlowBridgeStatus,
  GenerationMode,
  GenerationProgress,
  GenerationSettings,
  GenerationSlot,
  ProjectSummary,
  UpdateMetadata,
  UpdateStatus,
} from "../types";

interface SlotUpdateEvent extends Omit<Partial<GenerationSlot>, "assetType" | "status"> {
  localProjectId?: string | null;
  sourceOrder: number;
  status?: string;
  assetType?: GenerationSlot["assetType"] | "png" | "mp4";
  eventStatus?: "COMMAND_OK" | "COMMAND_FAILED";
  url?: string | null;
}

export function useAppController() {
  const [activeSection, setActiveSection] = useState("central");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [appInteractive, setAppInteractive] = useState(() => (
    typeof document === "undefined"
      ? true
      : document.visibilityState === "visible" && document.hasFocus()
  ));
  const [desktopReady, setDesktopReady] = useState(false);
  const [busy, setBusy] = useState("");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<UpdateMetadata | null>(null);
  const [toast, setToast] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<ProjectSummary | null>(null);
  const [title, setTitle] = useState("");
  const [maxWords, setMaxWords] = useState(7);
  const [pauseMs, setPauseMs] = useState(100);
  const [transitionMode, setTransitionMode] = useState("midpoint");
  const [promptText, setPromptText] = useState("");
  const [generationMode, setGenerationMode] = useState<GenerationMode>("IMAGE_TO_VIDEO");
  const [generationSettings, setGenerationSettings] = useState<GenerationSettings>({
    imageModel: "GEM_PIX_2",
    videoModel: "veo_3_1_t2v_lite_low_priority",
    i2vModel: "veo_3_1_i2v_lite_low_priority",
    imageAspectRatio: "IMAGE_ASPECT_RATIO_LANDSCAPE",
    videoAspectRatio: "VIDEO_ASPECT_RATIO_LANDSCAPE",
  });
  const [generationProgress, setGenerationProgress] = useState<GenerationProgress | null>(null);
  const [authenticated, setAuthenticated] = useState(false);
  const [authChecking, setAuthChecking] = useState(true);
  const [authToken, setAuthToken] = useState("");
  const [authError, setAuthError] = useState("");
  const [bridgeStatus, setBridgeStatus] = useState<FlowBridgeStatus>(initialBridgeStatus);
  const [assemblyStatus, setAssemblyStatus] = useState<AssemblyAiStatus>(initialAssemblyStatus);
  const [detailProjectId, setDetailProjectId] = useState<string | null>(null);
  const [downloadedAssets, setDownloadedAssets] = useState<DownloadedAsset[]>([]);
  const [generationSlots, setGenerationSlots] = useState<GenerationSlot[]>([]);
  const [failedLocalAssets, setFailedLocalAssets] = useState<Record<string, true>>({});
  const [failedRemoteAssets, setFailedRemoteAssets] = useState<Record<string, true>>({});
  const [inlineImageSrc, setInlineImageSrc] = useState<Record<string, string>>({});
  const [failedVideoThumbnails, setFailedVideoThumbnails] = useState<Record<string, true>>({});
  const [activeVideoSlot, setActiveVideoSlot] = useState<number | null>(null);
  const [videoThumbnailSrc, setVideoThumbnailSrc] = useState<Record<number, string>>({});
  const [videoPlaybackSrc, setVideoPlaybackSrc] = useState<Record<number, string>>({});
  const [pendingAudioByProject, setPendingAudioByProject] = useState<Record<string, string>>({});
  const detailRefreshInFlightRef = useRef(false);
  const detailRefreshQueuedRef = useRef(false);
  const detailRefreshTimerRef = useRef<number | null>(null);

  const selected = projects.find((project) => project.localProjectId === selectedId) ?? projects[0] ?? null;
  const selectedGenerationProgress = selected && generationProgress?.localProjectId === selected.localProjectId
    ? generationProgress
    : null;
  const selectedDownloadedAssets = selected && detailProjectId === selected.localProjectId ? downloadedAssets : [];
  const selectedGenerationSlots = selected && detailProjectId === selected.localProjectId ? generationSlots : [];
  const promptCount = useMemo(() => splitPrompts(promptText).length, [promptText]);
  const visibleSlots = useMemo(() => {
    const downloadedByOrder = new Map<number, DownloadedAsset>();
    for (const asset of selectedDownloadedAssets) {
      const order = slotOrderFromFilename(asset.filename);
      if (order != null) downloadedByOrder.set(order, asset);
    }

    if (selectedGenerationSlots.length > 0) {
      const mapped = selectedGenerationSlots.map((slot) => {
        const downloaded = downloadedByOrder.get(slot.sourceOrder);
        if (!downloaded) return normalizeGenerationSlot(slot);
        return normalizeGenerationSlot({
          ...slot,
          currentFileType: downloaded.fileType,
          localPath: downloaded.fullPath,
          status: slot.status === "queued" || slot.status === "processing" ? "ready" : slot.status,
        });
      });
      const existingOrders = new Set(mapped.map((slot) => slot.sourceOrder));
      const downloadedOnly = selectedDownloadedAssets
        .filter((asset) => {
          const order = slotOrderFromFilename(asset.filename);
          return order != null && !existingOrders.has(order);
        })
        .map((asset, index) => normalizeGenerationSlot({
          slotId: `slot_${String(slotOrderFromFilename(asset.filename) ?? index + 1).padStart(4, "0")}`,
          sourceOrder: slotOrderFromFilename(asset.filename) ?? index + 1,
          prompt: asset.filename,
          status: "ready",
          assetType: asset.fileType,
          activeAttemptId: null,
          attemptCount: 0,
          attempts: [],
          commandId: null,
          workflowId: null,
          batchId: null,
          operationId: null,
          thumbnailUrl: null,
          remoteStatus: "LOCAL_READY",
          remainingCredits: null,
          remoteUpdatedAt: null,
          currentFileType: asset.fileType,
          localPath: asset.fullPath,
          remoteUrl: null,
          mediaId: null,
          error: null,
        }));
      return [...mapped, ...downloadedOnly].sort((left, right) => left.sourceOrder - right.sourceOrder);
    }

    return selectedDownloadedAssets.map((asset, index) => normalizeGenerationSlot({
      slotId: `slot_${String(slotOrderFromFilename(asset.filename) ?? index + 1).padStart(4, "0")}`,
      sourceOrder: slotOrderFromFilename(asset.filename) ?? index + 1,
      prompt: asset.filename,
      status: "ready",
      assetType: asset.fileType,
      activeAttemptId: null,
      attemptCount: 0,
      attempts: [],
      commandId: null,
      workflowId: null,
      batchId: null,
      operationId: null,
      thumbnailUrl: null,
      remoteStatus: "LOCAL_READY",
      remainingCredits: null,
      remoteUpdatedAt: null,
      currentFileType: asset.fileType,
      localPath: asset.fullPath,
      remoteUrl: null,
      mediaId: null,
      error: null,
    }));
  }, [selectedDownloadedAssets, selectedGenerationSlots]);
  const animatableSourceOrders = useMemo(
    () => visibleSlots.filter(canAnimateSlot).map((slot) => slot.sourceOrder),
    [visibleSlots],
  );
  const readyLocalSlotCount = useMemo(
    () => visibleSlots.filter(hasFinalLocalAsset).length,
    [visibleSlots],
  );
  const hasActiveSlots = useMemo(
    () => selectedGenerationSlots.some((slot) => slot.status === "queued" || slot.status === "processing"),
    [selectedGenerationSlots],
  );
  const resumableGeneration = useMemo(() => {
    const total = Math.max(
      selected ? projectPromptTotal(selected) : 0,
      selectedGenerationSlots.length,
      visibleSlots.length,
    );
    const completed = visibleSlots.filter(hasFinalLocalAsset).length;
    const failed = selectedGenerationSlots.filter((slot) => slot.status === "failed").length;
    const processing = selectedGenerationSlots.filter((slot) => slot.status === "queued" || slot.status === "processing").length;
    const started = selectedGenerationSlots.length > 0 || completed > 0 || failed > 0 || processing > 0;
    return {
      total,
      completed,
      failed,
      processing,
      remaining: Math.max(total - completed, 0),
      resumable: started && total > 0 && completed < total,
    };
  }, [selected, selectedGenerationSlots, visibleSlots]);

  const updateBridgeStatus = (next: FlowBridgeStatus) => {
    setBridgeStatus((current) => sameBridgeStatus(current, next) ? current : next);
  };

  const applyProjectDetail = (localProjectId: string, detail: Awaited<ReturnType<typeof getProjectDetail>>) => {
    setDetailProjectId(localProjectId);
    setPromptText(detail.prompts.map((assignment) => assignment.prompt).join("\n"));
    setDownloadedAssets(detail.downloadedAssets ?? []);
    setGenerationSlots((detail.generationSlots ?? []).map((slot) => normalizeGenerationSlot(slot)));
  };

  const refreshSelectedProjectRuntime = async () => {
    if (!authenticated || !selected?.projectRoot) return;
    if (detailRefreshInFlightRef.current) {
      detailRefreshQueuedRef.current = true;
      return;
    }
    detailRefreshInFlightRef.current = true;
    const localProjectId = selected.localProjectId;
    const projectRoot = selected.projectRoot;
    try {
      const detail = await getProjectDetail(projectRoot);
      applyProjectDetail(localProjectId, detail);
    } catch {
      // Ignore transient refresh failures.
    } finally {
      detailRefreshInFlightRef.current = false;
      if (detailRefreshQueuedRef.current) {
        detailRefreshQueuedRef.current = false;
        window.setTimeout(() => {
          void refreshSelectedProjectRuntime();
        }, 150);
      }
    }
  };

  const scheduleSelectedProjectRuntimeRefresh = (delayMs = 150) => {
    if (detailRefreshTimerRef.current != null) {
      window.clearTimeout(detailRefreshTimerRef.current);
    }
    detailRefreshTimerRef.current = window.setTimeout(() => {
      detailRefreshTimerRef.current = null;
      void refreshSelectedProjectRuntime();
    }, delayMs);
  };

  const applySlotUpdate = (update: SlotUpdateEvent) => {
    setGenerationSlots((current) => {
      const index = current.findIndex((slot) => slot.sourceOrder === update.sourceOrder);
      if (index < 0) return current;

      const slot = current[index];
      const {
        assetType,
        eventStatus,
        localProjectId: _localProjectId,
        status: signalStatus,
        url,
        ...slotFields
      } = update;
      const nextStatus = eventStatus === "COMMAND_FAILED"
        ? "failed"
        : ["ready", "failed", "image-ready", "queued", "processing"].includes(signalStatus ?? "")
          ? signalStatus!
          : signalStatus === "IMAGE_READY"
            ? "image-ready"
            : "processing";
      const remoteAssetType = assetType === "mp4"
        ? "video"
        : assetType === "png"
          ? "image"
          : assetType;
      const next = normalizeGenerationSlot({
        ...slot,
        ...slotFields,
        status: nextStatus,
        assetType: remoteAssetType ?? slot.assetType,
        currentFileType: slotFields.currentFileType
          ?? (assetType === "mp4" ? "video" : assetType === "png" ? "image" : slot.currentFileType),
        remoteUrl: slotFields.remoteUrl ?? url ?? slot.remoteUrl,
        remoteUpdatedAt: new Date().toISOString(),
      });
      const updated = [...current];
      updated[index] = next;
      return updated;
    });
  };

  const refreshProjects = async (preferId?: string) => {
    try {
      const next = await listProjects();
      setProjects(next);
      const nextId = preferId ?? selectedId ?? next[0]?.localProjectId ?? null;
      setSelectedId(next.some((project) => project.localProjectId === nextId) ? nextId : next[0]?.localProjectId ?? null);
    } catch (error) {
      setToast(messageFrom(error));
    }
  };

  const refreshUpdateStatus = async () => {
    try {
      const status = await getUpdateStatus();
      setUpdateStatus(status);
      if (!status?.configured) {
        setAvailableUpdate(null);
      }
      return status;
    } catch (error) {
      setToast(messageFrom(error));
      return null;
    }
  };

  const handleCheckForUpdates = async (silent = false) => {
    const status = updateStatus ?? await refreshUpdateStatus();
    if (!status?.configured) {
      if (!silent) setToast("Atualizações automáticas ainda não foram configuradas para esta versão.");
      return null;
    }
    try {
      setBusy("update-check");
      const update = await checkForUpdate();
      setAvailableUpdate(update);
      if (!silent) {
        setToast(update
          ? `Atualização ${update.version} disponível.`
          : "Você já está na versão mais recente.");
      }
      return update;
    } catch (error) {
      if (!silent) setToast(messageFrom(error));
      return null;
    } finally {
      setBusy("");
    }
  };

  const handleInstallUpdate = async () => {
    let pending = availableUpdate;
    if (!pending) {
      pending = await handleCheckForUpdates(true);
      if (!pending) {
        setToast("Nenhuma atualização pendente foi encontrada.");
        return;
      }
    }
    try {
      setBusy("update-install");
      setToast(`Baixando a atualização ${pending.version}. O aplicativo pode fechar para concluir.`);
      await installPendingUpdate();
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  useEffect(() => {
    getAuthStatus()
      .then(async (status) => {
        setAuthenticated(status);
        if (status) {
          const info = await initializeWorkspace();
          setDesktopReady(Boolean(info));
          await refreshProjects();
          const updateState = await refreshUpdateStatus();
          if (updateState?.configured) {
            void handleCheckForUpdates(true);
          }
        }
      })
      .catch((error) => setAuthError(messageFrom(error)))
      .finally(() => setAuthChecking(false));
  }, []);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(""), 4400);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    if (!authenticated || !selected?.projectRoot) {
      setPromptText("");
      setDownloadedAssets([]);
      setGenerationSlots([]);
      setFailedLocalAssets({});
      setFailedRemoteAssets({});
      setFailedVideoThumbnails({});
      setInlineImageSrc({});
      setVideoThumbnailSrc({});
      setVideoPlaybackSrc((current) => {
        Object.values(current).forEach((url) => URL.revokeObjectURL(url));
        return {};
      });
      setActiveVideoSlot(null);
      return;
    }
    setPromptText("");
    setDetailProjectId(null);
    setDownloadedAssets([]);
    setGenerationSlots([]);
    setFailedLocalAssets({});
    setFailedRemoteAssets({});
    setFailedVideoThumbnails({});
    setInlineImageSrc({});
    setVideoThumbnailSrc({});
    setVideoPlaybackSrc((current) => {
      Object.values(current).forEach((url) => URL.revokeObjectURL(url));
      return {};
    });
    setActiveVideoSlot(null);
    void refreshSelectedProjectRuntime();
  }, [authenticated, selected?.localProjectId, selected?.promptCount]);

  useEffect(() => {
    if (typeof document === "undefined" || typeof window === "undefined") return;
    const updateInteractiveState = () => {
      setAppInteractive(document.visibilityState === "visible" && document.hasFocus());
    };
    updateInteractiveState();
    document.addEventListener("visibilitychange", updateInteractiveState);
    window.addEventListener("focus", updateInteractiveState);
    window.addEventListener("blur", updateInteractiveState);
    return () => {
      document.removeEventListener("visibilitychange", updateInteractiveState);
      window.removeEventListener("focus", updateInteractiveState);
      window.removeEventListener("blur", updateInteractiveState);
    };
  }, []);

  useEffect(() => {
    if (!authenticated || !selected?.projectRoot) return;
    const shouldPoll =
      selected.stage === "GENERATING_ASSETS"
      || (selectedGenerationProgress?.active ?? false)
      || hasActiveSlots;
    if (!shouldPoll) return;
    const refreshDetail = () => {
      scheduleSelectedProjectRuntimeRefresh(0);
    };
    refreshDetail();
    const timer = window.setInterval(refreshDetail, appInteractive ? 4000 : 12000);
    return () => {
      window.clearInterval(timer);
    };
  }, [authenticated, selected?.projectRoot, selected?.localProjectId, selected?.stage, selectedGenerationProgress?.active, hasActiveSlots, appInteractive]);

  useEffect(() => {
    if (!authenticated) {
      setBridgeStatus(initialBridgeStatus);
      setUpdateStatus(null);
      setAvailableUpdate(null);
      return;
    }
    const refresh = async () => {
      updateBridgeStatus(await getFlowBridgeStatus());
    };
    void refresh();
    const timer = window.setInterval(refresh, appInteractive ? 6000 : 15000);
    return () => window.clearInterval(timer);
  }, [authenticated, appInteractive]);

  useEffect(() => {
    if (!authenticated || !bridgeStatus.extensionConnected || !selected || selected.flowProjectId || bridgeStatus.pendingCommand) return;
    ensureFlowProjectLink(selected.localProjectId).catch((error) => setToast(messageFrom(error)));
  }, [authenticated, bridgeStatus.extensionConnected, bridgeStatus.pendingCommand, selected?.localProjectId, selected?.flowProjectId]);

  useEffect(() => {
    if (bridgeStatus.lastCommandError) setToast(bridgeStatus.lastCommandError);
  }, [bridgeStatus.lastCommandError]);

  useEffect(() => {
    return () => {
      if (detailRefreshTimerRef.current != null) {
        window.clearTimeout(detailRefreshTimerRef.current);
      }
      Object.values(videoPlaybackSrc).forEach((url) => URL.revokeObjectURL(url));
    };
  }, [videoPlaybackSrc]);

  useEffect(() => {
    if (!selected) return;
    const imageSlots = visibleSlots.filter((slot) =>
      slot.currentFileType === "image"
      && Boolean(slot.localPath)
      && !inlineImageSrc[slot.localPath!]
      && !failedLocalAssets[slot.localPath!],
    );
    if (imageSlots.length === 0) return;
    let cancelled = false;
    const queue = imageSlots.slice(0, 10);
    const run = async () => {
      for (const slot of queue) {
        if (cancelled || !slot.localPath) return;
        try {
          const src = await readLocalImageDataUrl(slot.localPath);
          if (cancelled) return;
          setInlineImageSrc((current) => current[slot.localPath!] ? current : { ...current, [slot.localPath!]: src });
        } catch {
          if (cancelled) return;
          setFailedLocalAssets((current) => current[slot.localPath!] ? current : { ...current, [slot.localPath!]: true });
          recordDiagnostic("asset-preview-load-failed", {
            sourceOrder: slot.sourceOrder,
            localPath: slot.localPath,
            remoteUrl: slot.remoteUrl,
            assetType: slot.currentFileType ?? slot.assetType,
            status: slot.status,
          }, "warning");
        }
      }
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, [selected, visibleSlots, inlineImageSrc, failedLocalAssets]);

  useEffect(() => {
    if (!selected) return;
    const videoSlots = visibleSlots.filter((slot) =>
      slot.currentFileType === "video"
      && !videoThumbnailSrc[slot.sourceOrder]
      && !(slot.mediaId && failedVideoThumbnails[slot.mediaId]),
    );
    if (videoSlots.length === 0) return;
    let cancelled = false;
    const queue = videoSlots.slice(0, 8);
    const run = async () => {
      for (const slot of queue) {
        if (cancelled) return;
        try {
          const src = await getSlotVideoPreviewDataUrl(selected.localProjectId, slot.sourceOrder);
          if (cancelled) return;
          setVideoThumbnailSrc((current) => current[slot.sourceOrder] ? current : { ...current, [slot.sourceOrder]: src });
        } catch {
          if (slot.mediaId) {
            setFailedVideoThumbnails((current) => current[slot.mediaId!] ? current : { ...current, [slot.mediaId!]: true });
          }
        }
      }
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, [selected?.localProjectId, visibleSlots, videoThumbnailSrc, failedVideoThumbnails]);

  useEffect(() => {
    if (!isDesktopApp()) return;
    let unlistenBridge: (() => void) | undefined;
    let unlistenSlot: (() => void) | undefined;
    let unlistenProjectLinked: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<string>("flowcontent-bridge-log", (event) => {
        console.log(`[Chrome Bridge] ${event.payload}`);
      }).then((fn) => {
        unlistenBridge = fn;
      });
      listen<SlotUpdateEvent>("flowcontent-slot-updated", (event) => {
        if (!selected || event.payload?.localProjectId !== selected.localProjectId) return;
        applySlotUpdate(event.payload);
        const shouldReconcile =
          event.payload.eventStatus != null
          || event.payload.status === "ready"
          || event.payload.status === "failed"
          || Boolean(event.payload.localPath);
        if (shouldReconcile) scheduleSelectedProjectRuntimeRefresh(500);
      }).then((fn) => {
        unlistenSlot = fn;
      });
      listen<{ localProjectId: string; flowProjectId: string }>("flowcontent-project-linked", (event) => {
        const { localProjectId, flowProjectId } = event.payload ?? {};
        if (!localProjectId || !flowProjectId) return;
        setProjects((current) => current.map((project) => (
          project.localProjectId === localProjectId
            ? { ...project, flowProjectId }
            : project
        )));
      }).then((fn) => {
        unlistenProjectLinked = fn;
      });
    });
    return () => {
      if (unlistenBridge) unlistenBridge();
      if (unlistenSlot) unlistenSlot();
      if (unlistenProjectLinked) unlistenProjectLinked();
    };
  }, [selected?.localProjectId, selected?.projectRoot, authenticated]);

  useEffect(() => {
    if (!authenticated) {
      setAssemblyStatus(initialAssemblyStatus);
      return;
    }
    getAssemblyAiStatus().then(setAssemblyStatus).catch((error) => setToast(messageFrom(error)));
  }, [authenticated]);

  useEffect(() => {
    if (!authenticated) return;
    let active = true;
    const poll = async () => {
      try {
        const progress = await getGenerationProgress();
        if (active) setGenerationProgress(hasVisibleProgress(progress) ? progress : null);
      } catch {
        // Ignore polling errors.
      }
    };
    void poll();
    const fastPoll = selected?.stage === "GENERATING_ASSETS" || (generationProgress?.active ?? false);
    const timer = window.setInterval(
      poll,
      fastPoll
        ? (appInteractive ? 2500 : 6000)
        : (appInteractive ? 6000 : 15000),
    );
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [authenticated, appInteractive, selected?.stage, generationProgress?.active]);

  const handleAuthSubmit = async () => {
    if (!authToken.trim() || authChecking || busy) return;
    setBusy("auth");
    setAuthError("");
    try {
      const result = await validateLicense(authToken.trim());
        if (result.valid) {
          setAuthenticated(true);
          const info = await initializeWorkspace();
          setDesktopReady(Boolean(info));
          await refreshProjects();
        } else {
        setAuthError(result.message || "Chave de acesso inválida.");
      }
    } catch {
      try {
        await authenticate(authToken.trim());
        setAuthenticated(true);
        const info = await initializeWorkspace();
        setDesktopReady(Boolean(info));
        await refreshProjects();
      } catch (fallbackError) {
        setAuthError(messageFrom(fallbackError));
      }
    }
    setBusy("");
  };

  const handleCreate = async () => {
    try {
      setBusy("create");
      const project = await createProject(title, null);
      await refreshProjects(project.localProjectId);
      setTitle("");
      setCreateOpen(false);
      setActiveSection("sincronizacao");
      setToast("Produção criada. A ponte criará e vinculará o projeto Flow automaticamente.");
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      setBusy("delete");
      const deleted = await deleteProject(deleteTarget.localProjectId);
      if (!deleted) {
        setToast("A produção já não estava registrada.");
      } else {
        setToast(`Produção "${deleteTarget.title}" e seus arquivos locais foram excluídos.`);
      }
      setDeleteTarget(null);
      setSelectedId(null);
      await refreshProjects();
      setActiveSection("central");
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleAuthenticate = async () => {
    try {
      setBusy("auth");
      setAuthError("");
      await authenticate(authToken);
      const info = await initializeWorkspace();
      setDesktopReady(Boolean(info));
      setAuthenticated(true);
      setAuthToken("");
      await refreshProjects();
    } catch (error) {
      setAuthError(messageFrom(error));
    } finally {
      setBusy("");
      setAuthChecking(false);
    }
  };

  const handleLock = async () => {
    await lockApp();
    setAuthenticated(false);
    setDesktopReady(false);
    setProjects([]);
    setSelectedId(null);
    setPromptText("");
    setAuthToken("");
  };

  const handleChooseAudio = async () => {
    if (!selected) return;
    try {
      const audioPath = await chooseAudio();
      if (!audioPath) {
        return;
      }
      setPendingAudioByProject((current) => ({ ...current, [selected.localProjectId]: audioPath }));
      setToast("Áudio selecionado. Revise o arquivo e clique em Gerar SRT.");
    } catch (error) {
      setToast(messageFrom(error));
    }
  };

  const handleAudio = async () => {
    if (!selected) return;
    try {
      const audioPath = pendingAudioByProject[selected.localProjectId] ?? selected.audioPath;
      if (!audioPath) {
        setToast("Selecione um áudio antes de gerar os SRTs.");
        return;
      }
      setBusy("audio");
      setToast("Enviando áudio ao AssemblyAI. A transcrição pode levar alguns minutos.");
      const result = await processProjectAudio(
        selected.projectRoot,
        audioPath,
        maxWords,
        pauseMs,
        transitionMode,
      );
      await refreshProjects(selected.localProjectId);
      setPendingAudioByProject((current) => {
        const next = { ...current };
        delete next[selected.localProjectId];
        return next;
      });
      setToast(`${result.captionCount} legendas e ${result.assetCount} slots inteiros gerados.`);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const refreshBridge = async () => {
    try {
      updateBridgeStatus(await getFlowBridgeStatus());
    } catch (error) {
      setToast(messageFrom(error));
    }
  };

  const handleOpenFlowBrowser = async () => {
    try {
      setBusy("browser");
      const status = await openFlowBrowser(selected?.flowProjectId ?? null);
      updateBridgeStatus(status);
      setToast(status.extensionInstalled
        ? "Chrome dedicado aberto. Faça o login manualmente e mantenha a página Flow aberta."
        : "Instalação inicial: clique em Carregar sem compactação e selecione a pasta aberta pelo aplicativo.");
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handlePrompts = async () => {
    if (!selected) return;
    try {
      setBusy("prompts");
      const result = await importProjectPrompts(selected.projectRoot, splitPrompts(promptText));
      await refreshProjects(selected.localProjectId);
      setToast(`${result.count} prompts salvos nesta produção.`);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleDownloadSrt = async (kind: "captions" | "assets") => {
    if (!selected) return;
    try {
      const downloaded = await downloadProjectSrt(
        selected.projectRoot,
        kind,
        `${selected.title}-${kind === "captions" ? "legendas" : "assets"}.srt`,
      );
      if (downloaded) setToast("SRT salvo.");
    } catch (error) {
      setToast(messageFrom(error));
    }
  };

  const handleExportCapcut = async () => {
    if (!selected) return;
    try {
      setBusy("capcut");
      setToast("Montando draft do CapCut com os assets sincronizados...");
      const result = await exportCapcutProject(selected.projectRoot);
      await refreshProjects(selected.localProjectId);
      setToast(`Draft do CapCut criado em ${result.draftPath}`);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleContinueGeneration = async () => {
    if (!selected) return;
    try {
      setBusy("generate");
      const message = await queueProjectGeneration(
        selected.localProjectId,
        generationMode,
        generationSettings,
        null,
        "continue",
      );
      await refreshProjects(selected.localProjectId);
      const [detail, progress] = await Promise.all([
        getProjectDetail(selected.projectRoot).catch(() => null),
        getGenerationProgress().catch(() => null),
      ]);
      if (detail) applyProjectDetail(selected.localProjectId, detail);
      if (progress && progress.localProjectId === selected.localProjectId) {
        setGenerationProgress(hasVisibleProgress(progress) ? progress : null);
      }
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleRestartGeneration = async () => {
    if (!selected) return;
    try {
      setBusy("generate");
      const message = await queueProjectGeneration(
        selected.localProjectId,
        generationMode,
        generationSettings,
        null,
        "restart",
      );
      await refreshProjects(selected.localProjectId);
      const [detail, progress] = await Promise.all([
        getProjectDetail(selected.projectRoot).catch(() => null),
        getGenerationProgress().catch(() => null),
      ]);
      if (detail) applyProjectDetail(selected.localProjectId, detail);
      if (progress && progress.localProjectId === selected.localProjectId) {
        setGenerationProgress(hasVisibleProgress(progress) ? progress : null);
      }
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handlePauseGeneration = async () => {
    if (!selected) return;
    try {
      setBusy("pause-generation");
      const message = await pauseProjectGeneration(selected.localProjectId);
      const [detail, progress] = await Promise.all([
        getProjectDetail(selected.projectRoot).catch(() => null),
        getGenerationProgress().catch(() => null),
      ]);
      if (detail) applyProjectDetail(selected.localProjectId, detail);
      if (progress && progress.localProjectId === selected.localProjectId) {
        setGenerationProgress(hasVisibleProgress(progress) ? progress : null);
      }
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleRetry = async () => {
    if (!selected) return;
    try {
      setBusy("generate");
      const message = await retryFailedGenerations(selected.localProjectId);
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleAnimateAll = async () => {
    if (!selected || animatableSourceOrders.length === 0) return;
    try {
      setBusy("animate");
      const message = await queueProjectAnimation(selected.localProjectId, animatableSourceOrders, generationSettings);
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleAnimateSlot = async (sourceOrder: number) => {
    if (!selected) return;
    try {
      setBusy(`animate-${sourceOrder}`);
      const slot = visibleSlots.find((item) => item.sourceOrder === sourceOrder);
      if (!slot) {
        throw new Error(`Slot ${sourceOrder} não encontrado.`);
      }
      const message = canAnimateSlot(slot)
        ? await queueProjectAnimation(selected.localProjectId, [sourceOrder], generationSettings)
        : await queueProjectGeneration(selected.localProjectId, "IMAGE_TO_VIDEO", generationSettings, [sourceOrder], "continue");
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleRefreshSlotAsset = async (sourceOrder: number) => {
    if (!selected) return;
    try {
      setBusy(`refresh-${sourceOrder}`);
      const message = await reconcileProjectSlotAsset(selected.localProjectId, sourceOrder);
      setToast(message);
      const detail = await getProjectDetail(selected.projectRoot);
      applyProjectDetail(selected.localProjectId, detail);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleRetrySlot = async (slot: GenerationSlot) => {
    if (!selected) return;
    try {
      setBusy(`retry-${slot.sourceOrder}`);
      const message = canAnimateSlot(slot)
        ? await queueProjectAnimation(selected.localProjectId, [slot.sourceOrder], generationSettings)
        : await queueProjectGeneration(selected.localProjectId, generationMode, generationSettings, [slot.sourceOrder], "continue");
      setToast(message);
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const handleSaveAssemblyKeys = async (keys: string) => {
    try {
      setBusy("assembly");
      const status = await saveAssemblyAiKeys(keys);
      setAssemblyStatus(status);
      setToast(`${status.keyCount} chave${status.keyCount === 1 ? "" : "s"} da AssemblyAI configurada${status.keyCount === 1 ? "" : "s"}.`);
      return true;
    } catch (error) {
      setToast(messageFrom(error));
      return false;
    } finally {
      setBusy("");
    }
  };

  const handleClearAssemblyKeys = async () => {
    try {
      setBusy("assembly");
      setAssemblyStatus(await clearAssemblyAiKeys());
      setToast("Chaves da AssemblyAI removidas.");
    } catch (error) {
      setToast(messageFrom(error));
    } finally {
      setBusy("");
    }
  };

  const markLocalAssetFailure = (slot: GenerationSlot) => {
    if (!slot.localPath) return;
    setFailedLocalAssets((current) => current[slot.localPath!] ? current : { ...current, [slot.localPath!]: true });
    recordDiagnostic("asset-preview-load-failed", {
      sourceOrder: slot.sourceOrder,
      localPath: slot.localPath,
      remoteUrl: slot.remoteUrl,
      assetType: slot.currentFileType ?? slot.assetType,
      status: slot.status,
    }, "warning");
  };

  const markRemoteAssetFailure = (slot: GenerationSlot) => {
    if (!slot.remoteUrl) return;
    setFailedRemoteAssets((current) => current[slot.remoteUrl!] ? current : { ...current, [slot.remoteUrl!]: true });
    recordDiagnostic("asset-remote-preview-load-failed", {
      sourceOrder: slot.sourceOrder,
      localPath: slot.localPath,
      remoteUrl: slot.remoteUrl,
      assetType: slot.currentFileType ?? slot.assetType,
      status: slot.status,
    }, "warning");
  };

  const markVideoThumbnailFailure = (slot: GenerationSlot) => {
    const mediaId = slot.mediaId;
    if (!mediaId) return;
    setFailedVideoThumbnails((current) => current[mediaId] ? current : { ...current, [mediaId]: true });
  };

  const ensureVideoPlaybackSrc = async (slot: GenerationSlot) => {
    if (!slot.localPath) throw new Error("Slot sem arquivo local.");
    const existing = videoPlaybackSrc[slot.sourceOrder];
    if (existing) return existing;
    const payload = await readLocalVideoBlobPayload(slot.localPath);
    const binary = atob(payload.base64);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    const url = URL.createObjectURL(new Blob([bytes], { type: payload.mimeType }));
    setVideoPlaybackSrc((current) => ({ ...current, [slot.sourceOrder]: url }));
    return url;
  };

  return {
    activeSection,
    setActiveSection,
    projects,
    selected,
    setSelectedId,
    desktopReady,
    busy,
    updateStatus,
    availableUpdate,
    toast,
    createOpen,
    setCreateOpen,
    deleteTarget,
    setDeleteTarget,
    title,
    setTitle,
    maxWords,
    setMaxWords,
    pauseMs,
    setPauseMs,
    transitionMode,
    setTransitionMode,
    promptText,
    setPromptText,
    promptCount,
    generationMode,
    setGenerationMode,
    generationSettings,
    setGenerationSettings,
    generationProgress,
    selectedGenerationProgress,
    authenticated,
    authChecking,
    authToken,
    setAuthToken,
    authError,
    bridgeStatus,
    assemblyStatus,
    visibleSlots,
    selectedDownloadedAssets,
    selectedGenerationSlots,
    readyLocalSlotCount,
    resumableGeneration,
    animatableSourceOrders,
    activeVideoSlot,
    setActiveVideoSlot,
    inlineImageSrc,
    failedRemoteAssets,
    videoPlaybackSrc,
    videoThumbnailSrc,
    pendingAudioByProject,
    handleAuthSubmit,
    handleLock,
    handleCheckForUpdates,
    handleInstallUpdate,
    handleCreate,
    handleDelete,
    handleChooseAudio,
    handleAudio,
    refreshBridge,
    handleOpenFlowBrowser,
    handlePrompts,
    handleDownloadSrt,
    handleExportCapcut,
    handleContinueGeneration,
    handlePauseGeneration,
    handleRestartGeneration,
    handleRetry,
    handleAnimateAll,
    handleAnimateSlot,
    handleRefreshSlotAsset,
    handleRetrySlot,
    handleSaveAssemblyKeys,
    handleClearAssemblyKeys,
    markLocalAssetFailure,
    markRemoteAssetFailure,
    markVideoThumbnailFailure,
    ensureVideoPlaybackSrc,
  };
}
