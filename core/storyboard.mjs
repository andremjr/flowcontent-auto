import crypto from "node:crypto";

const STORYBOARD_VERSION = 1;
const ATTEMPT_STATES = new Set([
  "DRAFT",
  "AUTHORIZED",
  "DISPATCHED",
  "SCHEDULED",
  "PENDING",
  "ACTIVE",
  "SUCCESSFUL",
  "FAILED",
  "CANCELLED",
  "BLOCKED",
]);

function createId(prefix) {
  return `${prefix}_${crypto.randomUUID()}`;
}

function sceneCode(ordinal) {
  return `scene-${String(ordinal).padStart(4, "0")}`;
}

function requireSlot(storyboard, slotId) {
  const slot = storyboard.slots.find((item) => item.slotId === slotId);
  if (!slot) throw new Error("storyboard slot not found");
  return slot;
}

function requireAttempt(slot, attemptId) {
  const attempt = slot.attempts.find((item) => item.attemptId === attemptId);
  if (!attempt) throw new Error("generation attempt not found");
  return attempt;
}

export function createStoryboard({ localProjectId, flowProjectId, now = new Date().toISOString() }) {
  if (!localProjectId || !flowProjectId) {
    throw new TypeError("localProjectId and flowProjectId are required");
  }
  return {
    version: STORYBOARD_VERSION,
    localProjectId,
    flowProjectId,
    createdAt: now,
    updatedAt: now,
    nextOrdinal: 1,
    slots: [],
  };
}

export function appendPrompts(storyboard, prompts, { now = new Date().toISOString() } = {}) {
  if (!Array.isArray(prompts) || prompts.length === 0) {
    throw new TypeError("prompts must be a non-empty array");
  }

  const created = prompts.map((prompt) => {
    if (!prompt?.trim()) throw new TypeError("every prompt must be non-empty");
    const ordinal = storyboard.nextOrdinal++;
    const slot = {
      slotId: createId("slot"),
      sceneCode: sceneCode(ordinal),
      ordinal,
      sourceOrder: ordinal,
      prompt: prompt.trim(),
      createdAt: now,
      updatedAt: now,
      status: "READY",
      activeAttemptId: null,
      attempts: [],
    };
    storyboard.slots.push(slot);
    return slot;
  });
  storyboard.updatedAt = now;
  return created;
}

export function startAttempt(
  storyboard,
  slotId,
  { kind, configuration, now = new Date().toISOString() },
) {
  const slot = requireSlot(storyboard, slotId);
  const attempt = {
    attemptId: createId("attempt"),
    attemptNumber: slot.attempts.length + 1,
    kind,
    configuration: structuredClone(configuration),
    state: "DRAFT",
    createdAt: now,
    updatedAt: now,
    flow: {
      workflowId: null,
      mediaId: null,
      parentMediaId: configuration?.parentMediaId ?? null,
    },
    download: null,
    error: null,
  };

  slot.attempts.push(attempt);
  slot.activeAttemptId = attempt.attemptId;
  slot.status = "GENERATING";
  slot.updatedAt = now;
  storyboard.updatedAt = now;
  return attempt;
}

export function observeAttempt(
  storyboard,
  slotId,
  attemptId,
  { state, workflowId, mediaId, error, now = new Date().toISOString() },
) {
  if (!ATTEMPT_STATES.has(state)) throw new TypeError("attempt state is invalid");
  const slot = requireSlot(storyboard, slotId);
  const attempt = requireAttempt(slot, attemptId);

  attempt.state = state;
  attempt.updatedAt = now;
  if (workflowId) attempt.flow.workflowId = workflowId;
  if (mediaId) attempt.flow.mediaId = mediaId;
  if (error) attempt.error = structuredClone(error);

  if (state === "SUCCESSFUL") slot.status = "READY";
  if (state === "FAILED" || state === "CANCELLED" || state === "BLOCKED") {
    slot.status = "NEEDS_ATTENTION";
  }
  slot.updatedAt = now;
  storyboard.updatedAt = now;
  return attempt;
}

export function selectAttempt(
  storyboard,
  slotId,
  attemptId,
  { now = new Date().toISOString() } = {},
) {
  const slot = requireSlot(storyboard, slotId);
  const attempt = requireAttempt(slot, attemptId);
  if (attempt.state !== "SUCCESSFUL") {
    throw new Error("only a successful attempt can be selected");
  }
  slot.activeAttemptId = attemptId;
  slot.status = "READY";
  slot.updatedAt = now;
  storyboard.updatedAt = now;
  return attempt;
}

export function attachDownload(
  storyboard,
  slotId,
  attemptId,
  { relativePath, downloadedAt = new Date().toISOString() },
) {
  if (!relativePath || relativePath.startsWith("/") || /^[a-zA-Z]:/.test(relativePath)) {
    throw new TypeError("download path must be relative to the project");
  }
  const slot = requireSlot(storyboard, slotId);
  const attempt = requireAttempt(slot, attemptId);
  if (!attempt.flow.mediaId) throw new Error("attempt has no observed Flow mediaId");

  attempt.download = {
    relativePath,
    downloadedAt,
    sourceMediaId: attempt.flow.mediaId,
  };
  attempt.updatedAt = downloadedAt;
  storyboard.updatedAt = downloadedAt;
  return attempt.download;
}

export function createTimeline({
  localProjectId,
  srt,
  cues,
  now = new Date().toISOString(),
}) {
  if (!Array.isArray(cues)) throw new TypeError("cues must be an array");
  return {
    version: 1,
    localProjectId,
    createdAt: now,
    updatedAt: now,
    srt: {
      relativePath: srt.relativePath,
      sha256: srt.sha256,
    },
    cues: cues.map((cue, index) => ({
      cueId: cue.cueId || `cue-${String(index + 1).padStart(4, "0")}`,
      sourceOrder: index + 1,
      startMs: cue.startMs,
      endMs: cue.endMs,
      text: cue.text,
      assignments: [],
    })),
  };
}

export function assignSlotToCue(
  timeline,
  storyboard,
  cueId,
  slotId,
  { attemptId, now = new Date().toISOString() } = {},
) {
  const cue = timeline.cues.find((item) => item.cueId === cueId);
  if (!cue) throw new Error("timeline cue not found");
  const slot = requireSlot(storyboard, slotId);
  const selectedAttemptId = attemptId || slot.activeAttemptId;
  if (!selectedAttemptId) throw new Error("slot has no selected attempt");
  const attempt = requireAttempt(slot, selectedAttemptId);
  if (attempt.state !== "SUCCESSFUL") throw new Error("assigned attempt is not successful");

  cue.assignments.push({
    assignmentId: createId("assignment"),
    slotId,
    sceneCode: slot.sceneCode,
    attemptId: selectedAttemptId,
    flowMediaId: attempt.flow.mediaId,
    localPath: attempt.download?.relativePath ?? null,
    assignedAt: now,
  });
  timeline.updatedAt = now;
  return cue.assignments.at(-1);
}

export function orderedSlots(storyboard) {
  return [...storyboard.slots].sort((a, b) => a.ordinal - b.ordinal);
}
