import test from "node:test";
import assert from "node:assert/strict";
import { appendPrompts, createStoryboard } from "../core/storyboard.mjs";
import { parseSrt, suggestSequentialAssignments } from "../core/srt.mjs";

const sample = `1
00:00:00,000 --> 00:00:02,500
Primeira legenda

2
00:00:02,500 --> 00:00:05,000 position:50%
Segunda legenda
em duas linhas
`;

test("parses SRT cues while preserving their source order and timing", () => {
  const parsed = parseSrt(sample);
  assert.equal(parsed.cueCount, 2);
  assert.equal(parsed.durationMs, 5_000);
  assert.deepEqual(parsed.cues.map((cue) => [cue.cueId, cue.startMs, cue.endMs]), [
    ["cue-0001", 0, 2_500],
    ["cue-0002", 2_500, 5_000],
  ]);
  assert.equal(parsed.cues[1].text, "Segunda legenda\nem duas linhas");
  assert.equal(parsed.cues[1].settings, "position:50%");
});

test("SRT hash is stable across line-ending formats", () => {
  assert.equal(parseSrt(sample).sha256, parseSrt(sample.replaceAll("\n", "\r\n")).sha256);
});

test("sequential mapping is suggestion-only and preserves storyboard holes", () => {
  const storyboard = createStoryboard({
    localProjectId: "local-1",
    flowProjectId: "flow-1",
  });
  appendPrompts(storyboard, ["Cena 1", "Cena 2"]);
  storyboard.slots[0].status = "NEEDS_ATTENTION";
  const suggestions = suggestSequentialAssignments(storyboard, parseSrt(sample));

  assert.equal(suggestions[0].sceneCode, "scene-0001");
  assert.equal(suggestions[1].sceneCode, "scene-0002");
  assert.equal(suggestions.every((item) => item.suggestionOnly), true);
});

test("rejects an invalid cue interval instead of reordering it", () => {
  assert.throws(
    () =>
      parseSrt(`1
00:00:04,000 --> 00:00:02,000
Inválido`),
    /ends before it starts/,
  );
});
