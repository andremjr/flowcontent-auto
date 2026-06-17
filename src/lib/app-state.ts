import type {
  AssemblyAiStatus,
  FlowBridgeStatus,
  GenerationProgress,
  GenerationSlot,
  ProductionStage,
  ProjectSummary,
} from "../types";

export const stageCopy: Record<ProductionStage, { short: string; title: string; description: string }> = {
  AWAITING_AUDIO: { short: "01", title: "Áudio", description: "Envie a narração" },
  AWAITING_PROMPTS: { short: "02", title: "SRTs prontos", description: "Crie os prompts fora do app" },
  READY_FOR_FLOW: { short: "03", title: "Prompts", description: "Ordem validada" },
  GENERATING_ASSETS: { short: "04", title: "Flow", description: "Geração em andamento" },
};

export const sectionTitles: Record<string, string> = {
  central: "Dashboard",
  producoes: "Produções",
  sincronizacao: "Sincronização SRT",
  sessoes: "Configurações",
};

export const initialBridgeStatus: FlowBridgeStatus = {
  serverReady: false,
  browserOpened: false,
  extensionInstalled: false,
  extensionConnected: false,
  flowPageDetected: false,
  flowUrl: null,
  projectId: null,
  pageTitle: null,
  lastHeartbeatMs: null,
  chromeProfile: null,
  extensionPath: null,
  pendingCommand: null,
  lastCommandError: null,
};

export const initialAssemblyStatus: AssemblyAiStatus = { configured: false, keyCount: 0, maskedKeys: [] };

export function sameBridgeStatus(left: FlowBridgeStatus, right: FlowBridgeStatus) {
  const stableKeys: (keyof FlowBridgeStatus)[] = [
    "serverReady",
    "browserOpened",
    "extensionInstalled",
    "extensionConnected",
    "flowPageDetected",
    "flowUrl",
    "projectId",
    "pageTitle",
    "chromeProfile",
    "extensionPath",
    "pendingCommand",
    "lastCommandError",
  ];
  return stableKeys.every((key) => left[key] === right[key]);
}

export function splitPrompts(value: string) {
  return value.split(/\r?\n/).map((prompt) => prompt.trim()).filter(Boolean);
}

export function messageFrom(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function slotStatusLabel(status: string) {
  switch (status) {
    case "queued": return "Na fila";
    case "processing": return "Gerando";
    case "image-ready": return "Imagem pronta";
    case "ready": return "Pronto";
    case "failed": return "Falhou";
    default: return status;
  }
}

export function remoteStatusLabel(status?: string | null) {
  switch (status) {
    case "COMMAND_DISPATCHED": return "Comando enviado";
    case "REMOTE_SCHEDULED": return "Na fila do Flow";
    case "REMOTE_ACTIVE": return "Processando no Flow";
    case "REMOTE_IMAGE_READY": return "Imagem remota pronta";
    case "REMOTE_SUCCESSFUL": return "Render remoto pronto";
    case "MEDIA_GENERATION_STATUS_PENDING": return "Agendado no Flow";
    case "MEDIA_GENERATION_STATUS_ACTIVE": return "Processando no Flow";
    case "MEDIA_GENERATION_STATUS_PROCESSING": return "Processando no Flow";
    case "MEDIA_GENERATION_STATUS_SUCCESSFUL": return "Render remoto pronto";
    case "MEDIA_GENERATION_STATUS_FAILED": return "Falha remota";
    case "LOCAL_READY": return "Salvo localmente";
    case "REMOTE_FAILED": return "Falha remota";
    case "LOCAL_DOWNLOAD_FAILED": return "Falha ao salvar local";
    case "DISPATCH_FAILED": return "Falha ao enviar comando";
    case "LOCAL_QUEUED": return "Aguardando envio";
    default: return status ?? "Sem status remoto";
  }
}

export function activeAttempt(slot: GenerationSlot) {
  return (slot.attempts ?? []).find((attempt) => attempt.attemptId === slot.activeAttemptId) ?? null;
}

export function normalizeGenerationSlot(slot: Partial<GenerationSlot> & Pick<GenerationSlot, "sourceOrder" | "prompt" | "status" | "assetType">): GenerationSlot {
  return {
    slotId: slot.slotId ?? `slot_${String(slot.sourceOrder).padStart(4, "0")}`,
    sourceOrder: slot.sourceOrder,
    prompt: slot.prompt,
    status: slot.status,
    assetType: slot.assetType,
    activeAttemptId: slot.activeAttemptId ?? null,
    attemptCount: slot.attemptCount ?? 0,
    attempts: slot.attempts ?? [],
    commandId: slot.commandId ?? null,
    workflowId: slot.workflowId ?? null,
    batchId: slot.batchId ?? null,
    operationId: slot.operationId ?? null,
    thumbnailUrl: slot.thumbnailUrl ?? null,
    remoteStatus: slot.remoteStatus ?? null,
    remainingCredits: slot.remainingCredits ?? null,
    remoteUpdatedAt: slot.remoteUpdatedAt ?? null,
    currentFileType: slot.currentFileType ?? null,
    localPath: slot.localPath ?? null,
    remoteUrl: slot.remoteUrl ?? null,
    mediaId: slot.mediaId ?? null,
    imageMediaId: slot.imageMediaId ?? null,
    error: slot.error ?? null,
  };
}

export function canAnimateSlot(slot: GenerationSlot) {
  const isImage = slot.currentFileType === "image";
  const hasImageMediaId = Boolean(
    slot.imageMediaId
    || slot.mediaId
    || (slot.remoteUrl && /\/image\/[^/?]+/.test(slot.remoteUrl)),
  );
  const isBusy = slot.status === "processing" || slot.status === "queued";
  return isImage && hasImageMediaId && !isBusy;
}

export function canPlaySlot(slot: GenerationSlot) {
  const isImage = slot.currentFileType === "image";
  const isBusy = slot.status === "processing" || slot.status === "queued";
  return isImage && !isBusy;
}

export function hasFinalLocalAsset(slot: GenerationSlot) {
  if (!slot.localPath) return false;
  if (slot.assetType === "video") {
    return slot.currentFileType === "video";
  }
  return Boolean(slot.currentFileType);
}

export function slotOrderFromFilename(filename: string) {
  const match = filename.match(/^(\d+)/);
  return match ? Number(match[1]) : null;
}

export function projectPromptTotal(project: ProjectSummary) {
  return project.promptCount || project.assetCount;
}

export function hasVisibleProgress(progress: GenerationProgress | null) {
  return Boolean(progress && (progress.active || progress.completedPrompts > 0 || progress.failedSlots.length > 0));
}
