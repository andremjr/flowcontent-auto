import test from "node:test";
import assert from "node:assert/strict";
import { hydrateRemoteProject } from "../core/project-hydration.mjs";
import {
  appendPrompts,
  attachDownload,
  createStoryboard,
  observeAttempt,
  startAttempt,
} from "../core/storyboard.mjs";

function preparedStoryboard() {
  const storyboard = createStoryboard({
    localProjectId: "local-1",
    flowProjectId: "flow-1",
  });
  const [slot] = appendPrompts(storyboard, ["Cena inicial"]);
  const attempt = startAttempt(storyboard, slot.slotId, {
    kind: "IMAGE",
    configuration: { model: "NARWHAL" },
  });
  observeAttempt(storyboard, slot.slotId, attempt.attemptId, {
    state: "SUCCESSFUL",
    mediaId: "media-linked",
    workflowId: "workflow-linked",
  });
  return { storyboard, slot, attempt };
}

test("matches remote assets to narrative slots by observed mediaId", () => {
  const { storyboard, slot } = preparedStoryboard();
  const hydrated = hydrateRemoteProject(storyboard, {
    flowProjectId: "flow-1",
    title: "Projeto remoto",
    observedAt: "2026-06-12T16:00:00Z",
    media: [
      {
        name: "media-linked",
        workflowId: "workflow-linked",
        image: { generatedImage: { modelNameType: "NARWHAL" }, dimensions: { width: 1376, height: 768 } },
      },
    ],
  });

  assert.equal(hydrated.linkedAssets[0].narrativeLink.slotId, slot.slotId);
  assert.equal(hydrated.unassignedAssets.length, 0);
});

test("remote-only assets remain unassigned and do not change storyboard order", () => {
  const { storyboard } = preparedStoryboard();
  const hydrated = hydrateRemoteProject(storyboard, {
    flowProjectId: "flow-1",
    title: "Projeto remoto",
    observedAt: "2026-06-12T16:00:00Z",
    media: [
      {
        name: "media-other",
        video: { generatedVideo: { model: "veo_3_1_fast" }, dimensions: { length: "8s" } },
      },
    ],
  });

  assert.equal(hydrated.unassignedAssets.length, 1);
  assert.equal(storyboard.slots.length, 1);
  assert.equal(storyboard.slots[0].sceneCode, "scene-0001");
});

test("missing remote media preserves its narrative and local download references", () => {
  const { storyboard, slot, attempt } = preparedStoryboard();
  attachDownload(storyboard, slot.slotId, attempt.attemptId, {
    relativePath: "downloads/scene-0001/media-linked.png",
  });
  const hydrated = hydrateRemoteProject(storyboard, {
    flowProjectId: "flow-1",
    title: "Projeto remoto",
    observedAt: "2026-06-12T16:00:00Z",
    media: [],
  });

  assert.equal(hydrated.missingRemote.length, 1);
  assert.equal(hydrated.missingRemote[0].hasLocalDownload, true);
  assert.equal(storyboard.slots[0].attempts[0].download.relativePath, "downloads/scene-0001/media-linked.png");
});

test("refuses hydration from a different Flow project", () => {
  const { storyboard } = preparedStoryboard();
  assert.throws(
    () =>
      hydrateRemoteProject(storyboard, {
        flowProjectId: "different-flow-project",
        title: "Outro",
        observedAt: "2026-06-12T16:00:00Z",
        media: [],
      }),
    /does not match/,
  );
});
