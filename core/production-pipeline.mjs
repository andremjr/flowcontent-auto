import crypto from "node:crypto";

const PIPELINE_VERSION = 1;

function nowIso() {
  return new Date().toISOString();
}

function clone(value) {
  return structuredClone(value);
}

export function createProductionPipeline({
  localProjectId,
  flowProjectId = null,
  title,
  now = nowIso(),
}) {
  if (!localProjectId || !title?.trim()) {
    throw new TypeError("localProjectId and title are required");
  }
  return {
    version: PIPELINE_VERSION,
    localProjectId,
    flowProjectId,
    title: title.trim(),
    stage: "AWAITING_AUDIO",
    createdAt: now,
    updatedAt: now,
    audio: null,
    captionSrt: null,
    assetSrt: null,
    assetBlocks: [],
  };
}

export function applyAudioSegmentation(
  pipeline,
  {
    audioRelativePath,
    captionSrtRelativePath,
    assetSrtRelativePath,
    transcriptSha256,
    captionMaxWords,
    assetMaxDurationMs,
    assets,
    now = nowIso(),
  },
) {
  if (!Array.isArray(assets) || assets.length === 0) {
    throw new TypeError("asset segmentation must contain at least one block");
  }
  if (assetMaxDurationMs > 8_000) {
    throw new Error("asset blocks cannot exceed 8 seconds");
  }

  pipeline.audio = {
    relativePath: audioRelativePath,
    transcriptSha256,
  };
  pipeline.captionSrt = {
    relativePath: captionSrtRelativePath,
    maxWords: captionMaxWords,
  };
  pipeline.assetSrt = {
    relativePath: assetSrtRelativePath,
    maxDurationMs: assetMaxDurationMs,
    boundaryPriority: ["sentence", "comma", "space"],
  };
  pipeline.assetBlocks = assets.map((asset, index) => {
    const sourceOrder = index + 1;
    if (asset.end - asset.start > 8_000) {
      throw new Error(`asset block ${sourceOrder} exceeds 8 seconds`);
    }
    return {
      assetBlockId: asset.segment_id || `asset-${String(sourceOrder).padStart(4, "0")}`,
      sourceOrder,
      startMs: asset.start,
      endMs: asset.end,
      durationMs: asset.end - asset.start,
      transcriptText: asset.text,
      contextText: asset.context_text ?? asset.text,
      pauseGroupId: asset.group_id ?? null,
      partIndex: asset.part_index ?? 1,
      partCount: asset.part_count ?? 1,
      pauseBeforeMs: asset.pause_before_ms ?? 0,
      pauseAfterMs: asset.pause_after_ms ?? 0,
      speechStartMs: asset.speech_start ?? asset.start,
      speechEndMs: asset.speech_end ?? asset.end,
      requiresReview: Boolean(asset.requires_review),
      boundary: asset.boundary,
      prompt: null,
      status: "AWAITING_PROMPT",
      attempts: [],
      activeAttemptId: null,
    };
  });
  pipeline.stage = "AWAITING_PROMPTS";
  pipeline.updatedAt = now;
  return pipeline;
}

export function parsePromptList(input) {
  if (Array.isArray(input)) {
    return input.map((prompt) => String(prompt).trim()).filter(Boolean);
  }
  if (typeof input !== "string") throw new TypeError("prompt list must be a string or array");

  const trimmed = input.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith("[")) {
    const parsed = JSON.parse(trimmed);
    if (!Array.isArray(parsed)) throw new TypeError("JSON prompt list must be an array");
    return parsed.map((prompt) => String(prompt).trim()).filter(Boolean);
  }
  return trimmed.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
}

export function importOrderedPrompts(pipeline, input, { now = nowIso() } = {}) {
  const prompts = parsePromptList(input);
  if (prompts.length !== pipeline.assetBlocks.length) {
    throw new Error(
      `prompt count mismatch: expected ${pipeline.assetBlocks.length}, received ${prompts.length}`,
    );
  }

  const assignments = pipeline.assetBlocks.map((block, index) => ({
    assetBlockId: block.assetBlockId,
    prompt: prompts[index],
  }));

  for (const assignment of assignments) {
    const block = pipeline.assetBlocks.find((item) => item.assetBlockId === assignment.assetBlockId);
    block.prompt = assignment.prompt;
    block.status = "READY";
  }
  pipeline.stage = "READY_FOR_FLOW";
  pipeline.updatedAt = now;
  return assignments;
}

export function startAssetAttempt(
  pipeline,
  assetBlockId,
  { model, configuration = {}, now = nowIso() },
) {
  const block = pipeline.assetBlocks.find((item) => item.assetBlockId === assetBlockId);
  if (!block) throw new Error("asset block not found");
  if (!block.prompt) throw new Error("asset block has no imported prompt");

  const attempt = {
    attemptId: `attempt_${crypto.randomUUID()}`,
    attemptNumber: block.attempts.length + 1,
    model,
    configuration: clone(configuration),
    state: "DRAFT",
    workflowId: null,
    mediaId: null,
    downloadRelativePath: null,
    createdAt: now,
    updatedAt: now,
  };
  block.attempts.push(attempt);
  block.activeAttemptId = attempt.attemptId;
  block.status = "GENERATING";
  pipeline.stage = "GENERATING_ASSETS";
  pipeline.updatedAt = now;
  return attempt;
}

export function observeAssetAttempt(
  pipeline,
  assetBlockId,
  attemptId,
  { state, workflowId, mediaId, now = nowIso() },
) {
  const block = pipeline.assetBlocks.find((item) => item.assetBlockId === assetBlockId);
  if (!block) throw new Error("asset block not found");
  const attempt = block.attempts.find((item) => item.attemptId === attemptId);
  if (!attempt) throw new Error("asset attempt not found");

  attempt.state = state;
  attempt.updatedAt = now;
  if (workflowId) attempt.workflowId = workflowId;
  if (mediaId) attempt.mediaId = mediaId;
  if (state === "SUCCESSFUL") block.status = "READY";
  if (["FAILED", "CANCELLED", "BLOCKED"].includes(state)) block.status = "NEEDS_ATTENTION";
  pipeline.updatedAt = now;
  return attempt;
}

export function orderedAssetBlocks(pipeline) {
  return [...pipeline.assetBlocks].sort((a, b) => a.sourceOrder - b.sourceOrder);
}
