import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { recordDiagnostic, summarizeForDiagnostic } from "./diagnostics";
import type { AssemblyAiStatus, AudioProcessResult, CapcutExportResult, FlowBridgeStatus, GenerationMode, GenerationProgress, GenerationSettings, ProjectDetail, ProjectSummary, RuntimeInfo, UpdateMetadata, UpdateStatus } from "../types";

export function isDesktopApp() {
  return "__TAURI_INTERNALS__" in window;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const startedAt = performance.now();
  recordDiagnostic("ipc-start", { command, args: summarizeForDiagnostic(args) });
  try {
    const result = await rawInvoke<T>(command, args);
    recordDiagnostic("ipc-success", {
      command,
      durationMs: Math.round(performance.now() - startedAt),
    });
    return result;
  } catch (error) {
    recordDiagnostic("ipc-error", {
      command,
      durationMs: Math.round(performance.now() - startedAt),
      error,
    }, "error");
    throw error;
  }
}

export async function getAuthStatus(): Promise<boolean> {
  if (!isDesktopApp()) return false;
  return invoke<boolean>("get_auth_status");
}

export async function authenticate(token: string): Promise<boolean> {
  if (!isDesktopApp()) throw new Error("Abra o aplicativo desktop para autenticar.");
  return invoke<boolean>("authenticate", { token });
}

export async function validateLicense(key: string): Promise<{ valid: boolean; message: string }> {
  if (!isDesktopApp()) throw new Error("Abra o aplicativo desktop para validar a licença.");
  return invoke<{ valid: boolean; message: string }>("validate_license", { key });
}

export async function getSavedLicense(): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return invoke<string | null>("get_saved_license");
}

export async function lockApp(): Promise<boolean> {
  if (!isDesktopApp()) return true;
  return invoke<boolean>("lock_app");
}

export async function getRuntimeInfo(): Promise<RuntimeInfo | null> {
  if (!isDesktopApp()) return null;
  return invoke<RuntimeInfo>("get_runtime_info");
}

export async function initializeWorkspace(): Promise<RuntimeInfo | null> {
  if (!isDesktopApp()) return null;
  return invoke<RuntimeInfo>("initialize_workspace");
}

export async function getUpdateStatus(): Promise<UpdateStatus | null> {
  if (!isDesktopApp()) return null;
  return invoke<UpdateStatus>("get_update_status");
}

export async function checkForUpdate(): Promise<UpdateMetadata | null> {
  if (!isDesktopApp()) return null;
  return invoke<UpdateMetadata | null>("check_for_update");
}

export async function installPendingUpdate(): Promise<boolean> {
  if (!isDesktopApp()) return false;
  return invoke<boolean>("install_pending_update");
}

export async function listProjects(): Promise<ProjectSummary[]> {
  if (!isDesktopApp()) return [];
  return invoke<ProjectSummary[]>("list_projects");
}

export async function getProjectDetail(projectRoot: string): Promise<ProjectDetail> {
  return invoke<ProjectDetail>("get_project_detail", { projectRoot });
}

export async function getFlowBridgeStatus(): Promise<FlowBridgeStatus> {
  return invoke<FlowBridgeStatus>("get_flow_bridge_status");
}

export async function openFlowBrowser(flowProjectId: string | null): Promise<FlowBridgeStatus> {
  return invoke<FlowBridgeStatus>("open_flow_browser", { flowProjectId });
}

export async function getAssemblyAiStatus(): Promise<AssemblyAiStatus> {
  return invoke<AssemblyAiStatus>("get_assemblyai_status");
}

export async function saveAssemblyAiKeys(keys: string): Promise<AssemblyAiStatus> {
  return invoke<AssemblyAiStatus>("save_assemblyai_keys", { keys });
}

export async function clearAssemblyAiKeys(): Promise<AssemblyAiStatus> {
  return invoke<AssemblyAiStatus>("clear_assemblyai_keys");
}

export async function createProject(title: string, flowProjectId: string | null): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("create_project", { title, flowProjectId });
}

export async function createProjectWithOptions(
  title: string,
  flowProjectId: string | null,
  assetOutputDir: string | null,
): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("create_project", { title, flowProjectId, assetOutputDir });
}

export async function deleteProject(localProjectId: string): Promise<boolean> {
  return invoke<boolean>("delete_project", { localProjectId });
}

export async function syncFlowProjectLinks(): Promise<number> {
  return invoke<number>("sync_flow_project_links");
}

export async function ensureFlowProjectLink(localProjectId: string): Promise<string | null> {
  return invoke<string | null>("ensure_flow_project_link", { localProjectId });
}

export async function queueProjectGeneration(
  localProjectId: string,
  mode: GenerationMode,
  settings: GenerationSettings,
  maxConcurrent: number,
  sourceOrders?: number[] | null,
  strategy: "continue" | "restart" = "continue",
): Promise<string> {
  return invoke<string>("queue_project_generation", {
    localProjectId,
    mode,
    imageModel: settings.imageModel,
    videoModel: settings.videoModel,
    i2vModel: settings.i2vModel,
    imageAspectRatio: settings.imageAspectRatio,
    videoAspectRatio: settings.videoAspectRatio,
    maxConcurrent,
    sourceOrders,
    generationStrategy: strategy,
  });
}

export async function queueProjectAnimation(
  localProjectId: string,
  sourceOrders: number[] | null,
  settings: GenerationSettings,
  maxConcurrent: number,
): Promise<string> {
  return invoke<string>("queue_project_animation", {
    localProjectId,
    sourceOrders,
    i2vModel: settings.i2vModel,
    videoAspectRatio: settings.videoAspectRatio,
    maxConcurrent,
  });
}

export async function getGenerationProgress(): Promise<GenerationProgress> {
  return invoke<GenerationProgress>("get_generation_progress");
}

export async function pauseProjectGeneration(localProjectId: string): Promise<string> {
  return invoke<string>("pause_project_generation", { localProjectId });
}

export async function reconcileProjectSlotAsset(localProjectId: string, sourceOrder: number): Promise<string> {
  return invoke<string>("reconcile_project_slot_asset", { localProjectId, sourceOrder });
}

export async function readLocalImageDataUrl(localPath: string): Promise<string> {
  return invoke<string>("read_local_image_data_url", { localPath });
}

export async function getSlotVideoPreviewDataUrl(localProjectId: string, sourceOrder: number): Promise<string> {
  return invoke<string>("get_slot_video_preview_data_url", { localProjectId, sourceOrder });
}

export async function readLocalVideoBlobPayload(localPath: string): Promise<{ mimeType: string; base64: string }> {
  return invoke<{ mimeType: string; base64: string }>("read_local_video_blob_payload", { localPath });
}

export async function retryFailedGenerations(localProjectId: string): Promise<string> {
  return invoke<string>("retry_failed_generations", { localProjectId });
}

export async function downloadProjectSrt(projectRoot: string, kind: "captions" | "assets", suggestedName: string) {
  const targetPath = await save({
    defaultPath: suggestedName,
    filters: [{ name: "Arquivo SRT", extensions: ["srt"] }],
  });
  if (!targetPath) return false;
  await invoke<boolean>("export_project_srt", { projectRoot, kind, targetPath });
  return true;
}

export async function exportCapcutProject(projectRoot: string): Promise<CapcutExportResult> {
  return invoke<CapcutExportResult>("export_capcut_project", { projectRoot });
}

export async function chooseAudio(): Promise<string | null> {
  const startedAt = performance.now();
  recordDiagnostic("file-dialog-open", { kind: "audio" });
  try {
    const selection = await open({
      multiple: false,
      directory: false,
      filters: [{
        name: "Áudio e vídeo",
        extensions: ["mp3", "wav", "mp4", "m4a", "aac", "flac", "ogg"],
      }],
    });
    recordDiagnostic("file-dialog-close", {
      kind: "audio",
      selected: typeof selection === "string",
      durationMs: Math.round(performance.now() - startedAt),
    });
    return typeof selection === "string" ? selection : null;
  } catch (error) {
    recordDiagnostic("file-dialog-error", {
      kind: "audio",
      durationMs: Math.round(performance.now() - startedAt),
      error,
    }, "error");
    throw error;
  }
}

export async function chooseDirectory(defaultPath?: string | null): Promise<string | null> {
  const startedAt = performance.now();
  recordDiagnostic("file-dialog-open", { kind: "directory" });
  try {
    const selection = await open({
      multiple: false,
      directory: true,
      defaultPath: defaultPath ?? undefined,
    });
    recordDiagnostic("file-dialog-close", {
      kind: "directory",
      selected: typeof selection === "string",
      durationMs: Math.round(performance.now() - startedAt),
    });
    return typeof selection === "string" ? selection : null;
  } catch (error) {
    recordDiagnostic("file-dialog-error", {
      kind: "directory",
      durationMs: Math.round(performance.now() - startedAt),
      error,
    }, "error");
    throw error;
  }
}

export async function processProjectAudio(
  projectRoot: string,
  audioPath: string,
  assetMode: string,
  assetValue: number,
  transitionMode: string,
): Promise<AudioProcessResult> {
  return invoke<AudioProcessResult>("process_audio", {
    projectRoot,
    audioPath,
    assetMode,
    assetValue,
    transitionMode,
  });
}

export async function importProjectPrompts(projectRoot: string, prompts: string[]) {
  return invoke<{ count: number; promptPath: string }>("import_prompts", { projectRoot, prompts });
}
