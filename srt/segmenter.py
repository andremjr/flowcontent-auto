#!/usr/bin/env python3
"""Pure subtitle and asset-segment generation from timestamped words."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from functools import lru_cache
import hashlib
import json
import math
import re
from typing import Iterable, Sequence

SENTENCE_END = re.compile(r'[.!?]+(?:["\')\]]+)?$')
COMMA_END = re.compile(r'[,;:]+(?:["\')\]]+)?$')
DEFAULT_PAUSE_THRESHOLD_MS = 100
MIN_ASSET_DURATION_MS = 3_000


@dataclass(frozen=True)
class Word:
    text: str
    start: int
    end: int


@dataclass(frozen=True)
class Segment:
    segment_id: str
    source_order: int
    start: int
    end: int
    text: str
    word_count: int
    boundary: str
    group_id: str | None = None
    part_index: int = 1
    part_count: int = 1
    context_text: str | None = None
    pause_before_ms: int = 0
    pause_after_ms: int = 0
    speech_start: int | None = None
    speech_end: int | None = None
    requires_review: bool = False

    @property
    def duration_ms(self) -> int:
        return self.end - self.start

    @property
    def speech_duration_ms(self) -> int:
        return int(self.speech_end or self.end) - int(self.speech_start or self.start)


def normalize_words(words: Iterable[object]) -> list[Word]:
    normalized: list[Word] = []
    for item in words:
        text = str(getattr(item, "text", "")).strip()
        start = int(getattr(item, "start"))
        end = int(getattr(item, "end"))
        if not text:
            continue
        if start < 0 or end <= start:
            raise ValueError(f"invalid timestamps for word: {text}")
        normalized.append(Word(text=text, start=start, end=end))

    if not normalized:
        raise ValueError("transcription contains no timestamped words")

    for previous, current in zip(normalized, normalized[1:]):
        if current.start < previous.start:
            raise ValueError("transcription words are not ordered")
    return normalized


def format_srt_time(milliseconds: int) -> str:
    milliseconds = max(0, int(milliseconds))
    hours, remainder = divmod(milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    seconds, millis = divmod(remainder, 1_000)
    return f"{hours:02}:{minutes:02}:{seconds:02},{millis:03}"


def _make_segment(
    prefix: str,
    source_order: int,
    words: Sequence[Word],
    boundary: str,
    *,
    group_id: str | None = None,
    part_index: int = 1,
    part_count: int = 1,
    context_text: str | None = None,
    pause_before_ms: int = 0,
    pause_after_ms: int = 0,
    visual_start: int | None = None,
    visual_end: int | None = None,
    requires_review: bool = False,
) -> Segment:
    return Segment(
        segment_id=f"{prefix}-{source_order:04d}",
        source_order=source_order,
        start=words[0].start if visual_start is None else visual_start,
        end=words[-1].end if visual_end is None else visual_end,
        text=" ".join(word.text for word in words),
        word_count=len(words),
        boundary=boundary,
        group_id=group_id,
        part_index=part_index,
        part_count=part_count,
        context_text=context_text,
        pause_before_ms=pause_before_ms,
        pause_after_ms=pause_after_ms,
        speech_start=words[0].start,
        speech_end=words[-1].end,
        requires_review=requires_review,
    )


def segment_captions(words: Sequence[Word], max_words: int) -> list[Segment]:
    if max_words < 1:
        raise ValueError("max_words must be positive")

    segments: list[Segment] = []
    for offset in range(0, len(words), max_words):
        chunk = words[offset : offset + max_words]
        segments.append(_make_segment("caption", len(segments) + 1, chunk, "word-limit"))
    return segments


def segment_assets_by_word_count(words: Sequence[Word], words_per_asset: int) -> list[Segment]:
    if words_per_asset < 1:
        raise ValueError("words_per_asset must be positive")

    segments: list[Segment] = []
    for offset in range(0, len(words), words_per_asset):
        chunk = words[offset : offset + words_per_asset]
        segments.append(_make_segment("asset", len(segments) + 1, chunk, "word-limit"))
    return segments


def segment_assets_by_duration(words: Sequence[Word], max_duration_ms: int) -> list[Segment]:
    if max_duration_ms < 1:
        raise ValueError("max_duration_ms must be positive")

    segments: list[Segment] = []
    start_index = 0
    while start_index < len(words):
        end_index = start_index
        while end_index + 1 < len(words):
            next_duration = words[end_index + 1].end - words[start_index].start
            if next_duration > max_duration_ms:
                break
            end_index += 1
        chunk = words[start_index : end_index + 1]
        segments.append(_make_segment("asset", len(segments) + 1, chunk, "duration-limit"))
        start_index = end_index + 1
    return segments


def pause_between(left: Word, right: Word) -> int:
    return max(0, right.start - left.end)


def pause_groups(
    words: Sequence[Word],
    pause_threshold_ms: int = DEFAULT_PAUSE_THRESHOLD_MS,
) -> list[tuple[int, int]]:
    if pause_threshold_ms < 0:
        raise ValueError("pause_threshold_ms cannot be negative")
    groups: list[tuple[int, int]] = []
    start_index = 0
    for index in range(len(words) - 1):
        if pause_between(words[index], words[index + 1]) >= pause_threshold_ms:
            groups.append((start_index, index))
            start_index = index + 1
    groups.append((start_index, len(words) - 1))
    return groups


def boundary_bonus(word: Word) -> int:
    if SENTENCE_END.search(word.text):
        return 350
    if COMMA_END.search(word.text):
        return 120
    return 0


def visual_boundary(left: Word, right: Word, transition_mode: str) -> int:
    if transition_mode == "midpoint":
        return left.end + pause_between(left, right) // 2
    if transition_mode == "next-speech":
        return right.start
    if transition_mode == "previous-speech":
        return left.end
    raise ValueError(f"invalid transition mode: {transition_mode}")


def coalesce_short_groups(
    groups: Sequence[tuple[int, int]],
    group_boundaries: Sequence[int],
    min_duration_ms: int,
    max_duration_ms: int,
) -> list[tuple[int, int, int, int]]:
    merged: list[tuple[int, int, int, int]] = []
    cursor = 0

    while cursor < len(groups):
        end = cursor
        while end + 1 < len(groups):
            current_duration = group_boundaries[end + 1] - group_boundaries[cursor]
            expanded_duration = group_boundaries[end + 2] - group_boundaries[cursor]
            if current_duration >= min_duration_ms or expanded_duration > max_duration_ms:
                break
            end += 1

        current_start, _ = groups[cursor]
        _, current_end = groups[end]
        current_visual_start = group_boundaries[cursor]
        current_visual_end = group_boundaries[end + 1]
        current_duration = current_visual_end - current_visual_start

        if merged:
            previous_start, _, previous_visual_start, _ = merged[-1]
            merged_duration = current_visual_end - previous_visual_start
            if current_duration < min_duration_ms and merged_duration <= max_duration_ms:
                merged[-1] = (
                    previous_start,
                    current_end,
                    previous_visual_start,
                    current_visual_end,
                )
            else:
                merged.append(
                    (
                        current_start,
                        current_end,
                        current_visual_start,
                        current_visual_end,
                    )
                )
        else:
            merged.append(
                (
                    current_start,
                    current_end,
                    current_visual_start,
                    current_visual_end,
                )
            )

        cursor = end + 1

    return merged


def partition_pause_group(
    words: Sequence[Word],
    part_count: int,
    max_duration_ms: int,
    min_duration_ms: int,
) -> list[tuple[int, int]]:
    if part_count == 1:
        if words[-1].end - words[0].start > max_duration_ms:
            raise ValueError("pause group cannot fit in one asset")
        return [(0, len(words) - 1)]

    ideal_duration = (words[-1].end - words[0].start) / part_count

    @lru_cache(maxsize=None)
    def solve(start_index: int, parts_left: int):
        if parts_left == 1:
            duration = words[-1].end - words[start_index].start
            if duration > max_duration_ms or duration < min_duration_ms:
                return None
            return (abs(duration - ideal_duration), ((start_index, len(words) - 1),))

        best = None
        latest_end_index = len(words) - parts_left
        for end_index in range(start_index, latest_end_index + 1):
            duration = words[end_index].end - words[start_index].start
            if duration > max_duration_ms:
                break
            if duration < min_duration_ms:
                continue

            remaining = solve(end_index + 1, parts_left - 1)
            if remaining is None:
                continue

            gap = pause_between(words[end_index], words[end_index + 1])
            # Time balance keeps every asset useful; pause size is the primary
            # boundary signal and punctuation is only a tie-breaker.
            cost = (
                abs(duration - ideal_duration)
                - min(gap, 2_000) * 1.8
                - boundary_bonus(words[end_index])
                + remaining[0]
            )
            candidate = (cost, ((start_index, end_index),) + remaining[1])
            if best is None or candidate[0] < best[0]:
                best = candidate
        return best

    result = solve(0, part_count)
    if result is None:
        if min_duration_ms <= 0:
            raise ValueError("pause group cannot be divided into assets of at most 8 seconds")
        return partition_pause_group(words, part_count, max_duration_ms, 0)
    return list(result[1])


def segment_assets(
    words: Sequence[Word],
    max_duration_ms: int = 8_000,
    pause_threshold_ms: int = DEFAULT_PAUSE_THRESHOLD_MS,
    transition_mode: str = "midpoint",
    timeline_start_ms: int | None = None,
    timeline_end_ms: int | None = None,
    min_duration_ms: int = MIN_ASSET_DURATION_MS,
) -> list[Segment]:
    if max_duration_ms < 1:
        raise ValueError("max_duration_ms must be positive")
    if min_duration_ms < 0:
        raise ValueError("min_duration_ms cannot be negative")
    if min_duration_ms > max_duration_ms:
        raise ValueError("min_duration_ms cannot exceed max_duration_ms")

    segments: list[Segment] = []
    pause_based_groups = pause_groups(words, pause_threshold_ms)
    pause_group_boundaries = [words[0].start if timeline_start_ms is None else timeline_start_ms]
    for previous_group, next_group in zip(pause_based_groups, pause_based_groups[1:]):
        pause_group_boundaries.append(
            visual_boundary(words[previous_group[1]], words[next_group[0]], transition_mode)
        )
    pause_group_boundaries.append(words[-1].end if timeline_end_ms is None else timeline_end_ms)
    groups = coalesce_short_groups(
        pause_based_groups,
        pause_group_boundaries,
        min_duration_ms,
        max_duration_ms,
    )

    for group_order, (group_start, group_end, group_visual_start, group_visual_end) in enumerate(
        groups,
        start=1,
    ):
        group_words = words[group_start : group_end + 1]
        group_visual_duration = group_visual_end - group_visual_start
        part_count = max(1, math.ceil(group_visual_duration / max_duration_ms))
        part_count = min(part_count, len(group_words))
        partitions = partition_pause_group(
            group_words,
            part_count,
            max_duration_ms,
            min_duration_ms,
        )
        context_text = " ".join(word.text for word in group_words)
        group_id = f"pause-{group_order:04d}"
        part_boundaries = [group_visual_start]
        for previous_part, next_part in zip(partitions, partitions[1:]):
            part_boundaries.append(
                visual_boundary(
                    group_words[previous_part[1]],
                    group_words[next_part[0]],
                    transition_mode,
                )
            )
        part_boundaries.append(group_visual_end)

        for part_index, (local_start, local_end) in enumerate(partitions, start=1):
            absolute_start = group_start + local_start
            absolute_end = group_start + local_end
            chunk = words[absolute_start : absolute_end + 1]
            pause_before = (
                pause_between(words[absolute_start - 1], words[absolute_start])
                if absolute_start > 0
                else 0
            )
            pause_after = (
                pause_between(words[absolute_end], words[absolute_end + 1])
                if absolute_end + 1 < len(words)
                else 0
            )
            boundary = "major-pause" if pause_after >= pause_threshold_ms else "micro-pause"
            visual_start = part_boundaries[part_index - 1]
            visual_end = part_boundaries[part_index]
            segments.append(
                _make_segment(
                    "asset",
                    len(segments) + 1,
                    chunk,
                    boundary,
                    group_id=group_id,
                    part_index=part_index,
                    part_count=part_count,
                    context_text=context_text,
                    pause_before_ms=pause_before,
                    pause_after_ms=pause_after,
                    visual_start=visual_start,
                    visual_end=visual_end,
                    requires_review=visual_end - visual_start > max_duration_ms,
                )
            )

    return segments


def render_srt(segments: Sequence[Segment]) -> str:
    entries: list[str] = []
    for index, segment in enumerate(segments, start=1):
        entries.extend(
            [
                str(index),
                f"{format_srt_time(segment.start)} --> {format_srt_time(segment.end)}",
                segment.text,
                "",
            ]
        )
    return "\n".join(entries)


def render_asset_srt(segments: Sequence[Segment]) -> str:
    entries: list[str] = []
    for index, segment in enumerate(segments, start=1):
        entries.extend(
            [
                str(index),
                f"{format_srt_time(segment.start)} --> {format_srt_time(segment.end)}",
                segment.text,
                "",
            ]
        )
    return "\n".join(entries)


def build_manifest(
    *,
    audio_path: str,
    words: Sequence[Word],
    captions: Sequence[Segment],
    assets: Sequence[Segment],
    max_words: int,
    max_asset_duration_ms: int,
    min_asset_duration_ms: int,
    pause_threshold_ms: int,
    transition_mode: str,
    asset_segmentation_mode: str,
    asset_segmentation_value: int,
    language_code: str | None,
) -> dict:
    transcript_text = " ".join(word.text for word in words)
    return {
        "version": 1,
        "audioPath": audio_path,
        "transcriptSha256": f"sha256:{hashlib.sha256(transcript_text.encode('utf-8')).hexdigest()}",
        "languageCode": language_code,
        "settings": {
            "captionMaxWords": max_words,
            "assetMaxDurationMs": max_asset_duration_ms,
            "assetMinDurationMs": min_asset_duration_ms,
            "pauseThresholdMs": pause_threshold_ms,
            "transitionMode": transition_mode,
            "assetSegmentationMode": asset_segmentation_mode,
            "assetSegmentationValue": asset_segmentation_value,
            "assetBoundaryPriority": ["pause-size", "time-balance", "punctuation-tiebreaker"],
        },
        "words": [asdict(word) for word in words],
        "captions": [{**asdict(segment), "duration_ms": segment.duration_ms} for segment in captions],
        "assets": [{**asdict(segment), "duration_ms": segment.duration_ms} for segment in assets],
    }


def write_manifest(filename: str, manifest: dict) -> None:
    with open(filename, "w", encoding="utf-8") as output:
        json.dump(manifest, output, ensure_ascii=False, indent=2)
        output.write("\n")
