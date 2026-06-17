import test from "node:test";
import assert from "node:assert/strict";
import {
  applyAudioSegmentation,
  createProductionPipeline,
  importOrderedPrompts,
  orderedAssetBlocks,
  startAssetAttempt,
} from "../core/production-pipeline.mjs";

function segmentedPipeline() {
  const pipeline = createProductionPipeline({
    localProjectId: "local-1",
    title: "Produção Aurora",
  });
  applyAudioSegmentation(pipeline, {
    audioRelativePath: "audio/narracao.mp3",
    captionSrtRelativePath: "srt/narracao.legendas.srt",
    assetSrtRelativePath: "srt/narracao.assets.srt",
    transcriptSha256: "sha256:transcript",
    captionMaxWords: 8,
    assetMaxDurationMs: 8_000,
    assets: [
      { segment_id: "asset-0001", start: 0, end: 5_000, text: "Primeiro bloco.", boundary: "sentence" },
      { segment_id: "asset-0002", start: 5_100, end: 12_000, text: "Segundo bloco", boundary: "space" },
    ],
  });
  return pipeline;
}

test("audio segmentation creates ordered prompt slots before prompts exist", () => {
  const pipeline = segmentedPipeline();
  assert.equal(pipeline.stage, "AWAITING_PROMPTS");
  assert.deepEqual(
    orderedAssetBlocks(pipeline).map((block) => [block.assetBlockId, block.prompt, block.status]),
    [
      ["asset-0001", null, "AWAITING_PROMPT"],
      ["asset-0002", null, "AWAITING_PROMPT"],
    ],
  );
});

test("prompt import is transactional when the count does not match", () => {
  const pipeline = segmentedPipeline();
  assert.throws(() => importOrderedPrompts(pipeline, "somente um prompt"), /expected 2, received 1/);
  assert.equal(pipeline.assetBlocks.every((block) => block.prompt === null), true);
  assert.equal(pipeline.stage, "AWAITING_PROMPTS");
});

test("prompt list maps one-to-one to asset blocks in source order", () => {
  const pipeline = segmentedPipeline();
  importOrderedPrompts(pipeline, "Prompt visual um\nPrompt visual dois");
  assert.deepEqual(
    pipeline.assetBlocks.map((block) => [block.assetBlockId, block.prompt]),
    [
      ["asset-0001", "Prompt visual um"],
      ["asset-0002", "Prompt visual dois"],
    ],
  );
  assert.equal(pipeline.stage, "READY_FOR_FLOW");
});

test("JSON prompt arrays preserve the same strict order", () => {
  const pipeline = segmentedPipeline();
  importOrderedPrompts(pipeline, JSON.stringify(["Prompt A", "Prompt B"]));
  assert.deepEqual(pipeline.assetBlocks.map((block) => block.prompt), ["Prompt A", "Prompt B"]);
});

test("Flow attempts can start only after the prompt for that asset block exists", () => {
  const pipeline = segmentedPipeline();
  assert.throws(
    () => startAssetAttempt(pipeline, "asset-0001", { model: "NARWHAL" }),
    /no imported prompt/,
  );
  importOrderedPrompts(pipeline, ["Prompt A", "Prompt B"]);
  const attempt = startAssetAttempt(pipeline, "asset-0001", { model: "NARWHAL" });
  assert.equal(attempt.attemptNumber, 1);
  assert.equal(pipeline.assetBlocks[0].sourceOrder, 1);
});
