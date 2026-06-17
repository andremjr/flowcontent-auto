export interface RuntimeInfo {
  appDataDir: string;
  documentsDir: string | null;
  workspaceRoot: string;
}

export interface UpdateStatus {
  enabled: boolean;
  configured: boolean;
  currentVersion: string;
  endpoints: string[];
}

export interface UpdateMetadata {
  version: string;
  currentVersion: string;
}

export type ProductionStage =
  | "AWAITING_AUDIO"
  | "AWAITING_PROMPTS"
  | "READY_FOR_FLOW"
  | "GENERATING_ASSETS";

export interface ProjectSummary {
  localProjectId: string;
  title: string;
  flowProjectId: string | null;
  projectRoot: string;
  stage: ProductionStage;
  assetCount: number;
  promptCount: number;
  captionSrtPath: string | null;
  assetSrtPath: string | null;
  audioPath: string | null;
  updatedAt: string;
}

export interface AudioProcessResult {
  audioPath: string;
  captionSrtPath: string;
  assetSrtPath: string;
  manifestPath: string;
  captionCount: number;
  assetCount: number;
  languageCode: string | null;
}

export interface CapcutExportResult {
  capcutRoot: string;
  draftPath: string;
  draftName: string;
  draftId: string;
  durationUs: number;
}

export interface AssetBlockDetail {
  segment_id: string;
  source_order: number;
  start: number;
  end: number;
  text: string;
  context_text?: string;
  part_index?: number;
  part_count?: number;
  requires_review?: boolean;
}

export interface PromptAssignment {
  sourceOrder: number;
  assetBlockId: string;
  prompt: string;
}

export interface DownloadedAsset {
  filename: string;
  fileType: "image" | "video";
  fileSize: number;
  fullPath: string;
}

export interface GenerationAttempt {
  attemptId: string;
  attemptNumber: number;
  kind: string;
  commandType: string;
  commandId: string | null;
  batchId: string | null;
  state: string;
  remoteStatus: string | null;
  workflowId: string | null;
  mediaId: string | null;
  imageMediaId: string | null;
  operationId: string | null;
  thumbnailUrl: string | null;
  remainingCredits: number | null;
  prompt: string;
  createdAt: string;
  updatedAt: string;
  error: string | null;
}

export interface GenerationSlot {
  slotId: string;
  sourceOrder: number;
  prompt: string;
  status: string;
  assetType: "image" | "video";
  activeAttemptId: string | null;
  attemptCount: number;
  attempts: GenerationAttempt[];
  commandId: string | null;
  workflowId: string | null;
  batchId: string | null;
  operationId: string | null;
  thumbnailUrl?: string | null;
  remoteStatus?: string | null;
  remainingCredits?: number | null;
  remoteUpdatedAt?: string | null;
  currentFileType: "image" | "video" | null;
  localPath: string | null;
  remoteUrl: string | null;
  mediaId: string | null;
  imageMediaId?: string | null;
  error: string | null;
}

export interface ProjectDetail {
  production: Record<string, unknown>;
  assets: AssetBlockDetail[];
  captions: Array<Record<string, unknown>>;
  settings: Record<string, unknown>;
  prompts: PromptAssignment[];
  generationSlots: GenerationSlot[];
  downloadedAssets: DownloadedAsset[];
}

export interface FlowBridgeStatus {
  serverReady: boolean;
  browserOpened: boolean;
  extensionInstalled: boolean;
  extensionConnected: boolean;
  flowPageDetected: boolean;
  flowUrl: string | null;
  projectId: string | null;
  pageTitle: string | null;
  lastHeartbeatMs: number | null;
  chromeProfile: string | null;
  extensionPath: string | null;
  pendingCommand: string | null;
  lastCommandError: string | null;
}

export type GenerationMode = "IMAGE" | "VIDEO" | "IMAGE_TO_VIDEO";

export interface AssemblyAiStatus {
  configured: boolean;
  keyCount: number;
  maskedKeys: string[];
}

export interface GenerationProgress {
  localProjectId: string | null;
  active: boolean;
  totalPrompts: number;
  completedPrompts: number;
  failedSlots: Array<{ sourceOrder: number; error: string }>;
  currentIndex: number;
  inFlight: number;
  paused: boolean;
}

export interface GenerationSettings {
  imageModel: string;
  videoModel: string;
  i2vModel: string;
  imageAspectRatio: string;
  videoAspectRatio: string;
}
