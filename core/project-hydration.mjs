function normalizeRemoteMedia(media) {
  const mediaId =
    media.mediaId ||
    media.name ||
    media.image?.generatedImage?.mediaId ||
    media.video?.generatedVideo?.mediaId;
  if (!mediaId) throw new TypeError("remote media has no observable mediaId");

  const kind = media.image ? "IMAGE" : media.video ? "VIDEO" : media.mediaType || "UNKNOWN";
  const generation =
    media.image?.generatedImage ||
    media.video?.generatedVideo ||
    media.audio?.generatedAudio ||
    {};

  return {
    mediaId,
    workflowId: media.workflowId || generation.workflowId || null,
    kind,
    status: media.mediaMetadata?.mediaStatus?.mediaGenerationStatus || "OBSERVED",
    model: generation.model || generation.modelNameType || null,
    prompt: generation.prompt || media.mediaMetadata?.mediaTitle || null,
    dimensions: media.image?.dimensions || media.video?.dimensions || null,
    remoteAvailable: true,
  };
}

function attemptIndex(storyboard) {
  const index = new Map();
  for (const slot of storyboard.slots) {
    for (const attempt of slot.attempts) {
      if (attempt.flow.mediaId) {
        index.set(attempt.flow.mediaId, {
          slotId: slot.slotId,
          sceneCode: slot.sceneCode,
          attemptId: attempt.attemptId,
          active: slot.activeAttemptId === attempt.attemptId,
          hasLocalDownload: Boolean(attempt.download),
        });
      }
    }
  }
  return index;
}

export function hydrateRemoteProject(storyboard, observedProject) {
  if (storyboard.flowProjectId !== observedProject.flowProjectId) {
    throw new Error("observed Flow project does not match the local project");
  }

  const linkedAttempts = attemptIndex(storyboard);
  const assets = observedProject.media.map(normalizeRemoteMedia).map((asset) => ({
    ...asset,
    narrativeLink: linkedAttempts.get(asset.mediaId) || null,
  }));
  const observedIds = new Set(assets.map((asset) => asset.mediaId));
  const missingRemote = [];

  for (const [mediaId, link] of linkedAttempts) {
    if (!observedIds.has(mediaId)) {
      missingRemote.push({
        mediaId,
        ...link,
        remoteAvailable: false,
      });
    }
  }

  return {
    flowProjectId: observedProject.flowProjectId,
    title: observedProject.title,
    hydratedAt: observedProject.observedAt,
    assets,
    linkedAssets: assets.filter((asset) => asset.narrativeLink),
    unassignedAssets: assets.filter((asset) => !asset.narrativeLink),
    missingRemote,
  };
}
