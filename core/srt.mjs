import crypto from "node:crypto";

const TIMECODE = /^(\d{2,}):(\d{2}):(\d{2})[,.](\d{3})$/;

function parseTimecode(value) {
  const match = value.trim().match(TIMECODE);
  if (!match) throw new Error(`invalid SRT timecode: ${value}`);
  const [, hours, minutes, seconds, milliseconds] = match.map(Number);
  if (minutes > 59 || seconds > 59) throw new Error(`invalid SRT timecode: ${value}`);
  return (((hours * 60 + minutes) * 60 + seconds) * 1000) + milliseconds;
}

function cueId(sourceOrder) {
  return `cue-${String(sourceOrder).padStart(4, "0")}`;
}

export function parseSrt(content) {
  if (typeof content !== "string" || !content.trim()) {
    throw new TypeError("SRT content must be a non-empty string");
  }

  const normalized = content.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n").trim();
  const blocks = normalized.split(/\n{2,}/);
  const cues = blocks.map((block, blockIndex) => {
    const lines = block.split("\n");
    const numericIndex = /^\d+$/.test(lines[0].trim()) ? Number(lines.shift().trim()) : null;
    const timing = lines.shift();
    const [startRaw, endWithSettings] = timing?.split(/\s+-->\s+/) ?? [];
    if (!startRaw || !endWithSettings) {
      throw new Error(`invalid SRT cue at block ${blockIndex + 1}`);
    }
    const [endRaw, ...settings] = endWithSettings.trim().split(/\s+/);
    const startMs = parseTimecode(startRaw);
    const endMs = parseTimecode(endRaw);
    if (endMs <= startMs) throw new Error(`SRT cue ends before it starts at block ${blockIndex + 1}`);

    const sourceOrder = blockIndex + 1;
    return {
      cueId: cueId(sourceOrder),
      sourceOrder,
      sourceIndex: numericIndex,
      startMs,
      endMs,
      durationMs: endMs - startMs,
      settings: settings.join(" ") || null,
      text: lines.join("\n").trim(),
    };
  });

  return {
    sha256: `sha256:${crypto.createHash("sha256").update(normalized).digest("hex")}`,
    cueCount: cues.length,
    durationMs: Math.max(...cues.map((cue) => cue.endMs)),
    cues,
  };
}

export function suggestSequentialAssignments(storyboard, parsedSrt) {
  const slots = [...storyboard.slots].sort((a, b) => a.ordinal - b.ordinal);
  return parsedSrt.cues.map((cue, index) => {
    const slot = slots[index] || null;
    return {
      cueId: cue.cueId,
      sourceOrder: cue.sourceOrder,
      slotId: slot?.slotId ?? null,
      sceneCode: slot?.sceneCode ?? null,
      suggestionOnly: true,
    };
  });
}
