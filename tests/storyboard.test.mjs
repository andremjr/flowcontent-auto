import test from "node:test";
import assert from "node:assert/strict";
import {
  appendPrompts,
  assignSlotToCue,
  attachDownload,
  createStoryboard,
  createTimeline,
  observeAttempt,
  orderedSlots,
  selectAttempt,
  startAttempt,
} from "../core/storyboard.mjs";

function storyboardWithPrompts() {
  const storyboard = createStoryboard({
    localProjectId: "local-1",
    flowProjectId: "flow-1",
  });
  appendPrompts(storyboard, ["Primeira cena", "Segunda cena", "Terceira cena"]);
  return storyboard;
}

test("a failed generation leaves an explicit hole without reordering later scenes", () => {
  const storyboard = storyboardWithPrompts();
  const second = storyboard.slots[1];
  const attempt = startAttempt(storyboard, second.slotId, {
    kind: "IMAGE",
    configuration: { model: "NARWHAL" },
  });
  observeAttempt(storyboard, second.slotId, attempt.attemptId, {
    state: "FAILED",
    error: { code: "GENERATION_FAILED" },
  });

  assert.deepEqual(
    orderedSlots(storyboard).map((slot) => [slot.sceneCode, slot.prompt, slot.status]),
    [
      ["scene-0001", "Primeira cena", "READY"],
      ["scene-0002", "Segunda cena", "NEEDS_ATTENTION"],
      ["scene-0003", "Terceira cena", "READY"],
    ],
  );
});

test("retry creates another attempt inside the same narrative slot", () => {
  const storyboard = storyboardWithPrompts();
  const slot = storyboard.slots[0];
  const first = startAttempt(storyboard, slot.slotId, {
    kind: "IMAGE",
    configuration: { model: "NARWHAL" },
  });
  observeAttempt(storyboard, slot.slotId, first.attemptId, { state: "FAILED" });
  const retry = startAttempt(storyboard, slot.slotId, {
    kind: "IMAGE",
    configuration: { model: "NARWHAL" },
  });
  observeAttempt(storyboard, slot.slotId, retry.attemptId, {
    state: "SUCCESSFUL",
    mediaId: "media-retry",
  });

  assert.equal(slot.sceneCode, "scene-0001");
  assert.equal(slot.attempts.length, 2);
  assert.equal(slot.activeAttemptId, retry.attemptId);
});

test("new prompts append without reusing or changing existing ordinals", () => {
  const storyboard = storyboardWithPrompts();
  storyboard.slots[1].status = "NEEDS_ATTENTION";
  appendPrompts(storyboard, ["Quarta cena"]);

  assert.deepEqual(orderedSlots(storyboard).map((slot) => slot.sceneCode), [
    "scene-0001",
    "scene-0002",
    "scene-0003",
    "scene-0004",
  ]);
});

test("timeline assignment snapshots the selected remote and local asset identity", () => {
  const storyboard = storyboardWithPrompts();
  const slot = storyboard.slots[0];
  const attempt = startAttempt(storyboard, slot.slotId, {
    kind: "VIDEO",
    configuration: { model: "veo_3_1_i2v_lite_low_priority" },
  });
  observeAttempt(storyboard, slot.slotId, attempt.attemptId, {
    state: "SUCCESSFUL",
    mediaId: "media-video-1",
    workflowId: "workflow-1",
  });
  selectAttempt(storyboard, slot.slotId, attempt.attemptId);
  attachDownload(storyboard, slot.slotId, attempt.attemptId, {
    relativePath: "downloads/scene-0001/media-video-1.mp4",
  });
  const timeline = createTimeline({
    localProjectId: "local-1",
    srt: { relativePath: "audio/video.srt", sha256: "sha256:srt" },
    cues: [{ startMs: 0, endMs: 2500, text: "Primeira legenda" }],
  });

  const assignment = assignSlotToCue(timeline, storyboard, "cue-0001", slot.slotId);

  assert.equal(assignment.sceneCode, "scene-0001");
  assert.equal(assignment.flowMediaId, "media-video-1");
  assert.equal(assignment.localPath, "downloads/scene-0001/media-video-1.mp4");
});
