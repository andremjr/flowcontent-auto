#!/usr/bin/env python3
"""Transcribe one audio file once and generate FlowContent Auto SRT artifacts."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import sys

import assemblyai as aai

from segmenter import (
    MIN_ASSET_DURATION_MS,
    DEFAULT_PAUSE_THRESHOLD_MS,
    build_manifest,
    normalize_words,
    render_asset_srt,
    render_srt,
    segment_assets,
    segment_captions,
    write_manifest,
)

BASE_DIR = Path(__file__).resolve().parent
DEFAULT_API_KEY_FILE = BASE_DIR / "chave-api-assemblyai.txt"
SUPPORTED_AUDIO_EXTENSIONS = {".mp3", ".wav", ".mp4", ".m4a", ".aac", ".flac", ".ogg"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate caption and asset SRT files from one audio.")
    parser.add_argument("--audio", required=True, help="Audio file to transcribe.")
    parser.add_argument("--project-root", required=True, help="FlowContent Auto project directory.")
    parser.add_argument("--max-words", type=int, default=8, help="Maximum words per caption cue.")
    parser.add_argument("--max-asset-ms", type=int, default=8000, help="Maximum duration of each asset cue.")
    parser.add_argument(
        "--pause-ms",
        type=int,
        default=DEFAULT_PAUSE_THRESHOLD_MS,
        help="Pause that starts a new narration unit.",
    )
    parser.add_argument(
        "--transition-mode",
        choices=["midpoint", "next-speech", "previous-speech"],
        default="midpoint",
        help="Where the visual asset changes inside a pause.",
    )
    parser.add_argument("--api-key-file", default=str(DEFAULT_API_KEY_FILE), help="AssemblyAI API key file.")
    parser.add_argument("--no-copy-audio", action="store_true", help="Do not copy audio into the project.")
    return parser.parse_args()


def read_api_keys(filename: Path) -> list[str]:
    if not filename.is_file():
        raise RuntimeError(f"AssemblyAI API key file not found: {filename}")
    keys = [line.strip() for line in filename.read_text(encoding="utf-8").splitlines() if line.strip()]
    if not keys:
        raise RuntimeError("AssemblyAI API key file is empty")
    return keys


def transcribe(audio_path: Path, api_keys: list[str]):
    config = aai.TranscriptionConfig(language_detection=True)
    errors: list[str] = []

    for key_index, api_key in enumerate(api_keys, start=1):
        aai.settings.api_key = api_key
        try:
            transcript = aai.Transcriber().transcribe(str(audio_path), config=config)
            if transcript.status == aai.TranscriptStatus.completed:
                return transcript
            errors.append(f"key #{key_index}: {transcript.error or transcript.status}")
        except Exception as error:
            errors.append(f"key #{key_index}: {error}")

    raise RuntimeError("AssemblyAI transcription failed: " + "; ".join(errors))


def main() -> int:
    args = parse_args()
    audio_path = Path(args.audio).resolve()
    project_root = Path(args.project_root).resolve()
    key_file = Path(args.api_key_file).resolve()

    if not audio_path.is_file():
        raise RuntimeError(f"audio file not found: {audio_path}")
    if audio_path.suffix.lower() not in SUPPORTED_AUDIO_EXTENSIONS:
        raise RuntimeError(f"unsupported audio extension: {audio_path.suffix}")
    if args.max_words < 1:
        raise RuntimeError("max words must be positive")
    if args.max_asset_ms < 1 or args.max_asset_ms > 8000:
        raise RuntimeError("asset maximum duration must be between 1 and 8000 ms")

    audio_dir = project_root / "audio"
    srt_dir = project_root / "srt"
    metadata_dir = project_root / ".flowcontent"
    audio_dir.mkdir(parents=True, exist_ok=True)
    srt_dir.mkdir(parents=True, exist_ok=True)
    metadata_dir.mkdir(parents=True, exist_ok=True)

    project_audio_path = audio_path
    if not args.no_copy_audio:
        project_audio_path = audio_dir / audio_path.name
        if project_audio_path != audio_path:
            shutil.copy2(audio_path, project_audio_path)

    transcript = transcribe(audio_path, read_api_keys(key_file))
    words = normalize_words(transcript.words)
    caption_segments = segment_captions(words, args.max_words)
    audio_duration = getattr(transcript, "audio_duration", None)
    timeline_end_ms = round(float(audio_duration) * 1000) if audio_duration else words[-1].end
    asset_segments = segment_assets(
        words,
        args.max_asset_ms,
        args.pause_ms,
        args.transition_mode,
        timeline_start_ms=0,
        timeline_end_ms=timeline_end_ms,
    )
    base_name = audio_path.stem

    caption_path = srt_dir / f"{base_name}.legendas.srt"
    asset_path = srt_dir / f"{base_name}.assets.srt"
    manifest_path = metadata_dir / "audio-segments.json"

    caption_path.write_text(render_srt(caption_segments), encoding="utf-8")
    asset_path.write_text(render_asset_srt(asset_segments), encoding="utf-8")
    manifest = build_manifest(
        audio_path=os.path.relpath(project_audio_path, project_root),
        words=words,
        captions=caption_segments,
        assets=asset_segments,
        max_words=args.max_words,
        max_asset_duration_ms=args.max_asset_ms,
        min_asset_duration_ms=MIN_ASSET_DURATION_MS,
        pause_threshold_ms=args.pause_ms,
        transition_mode=args.transition_mode,
        language_code=getattr(transcript, "language_code", None),
    )
    write_manifest(str(manifest_path), manifest)

    result = {
        "audioPath": str(project_audio_path),
        "captionSrtPath": str(caption_path),
        "assetSrtPath": str(asset_path),
        "manifestPath": str(manifest_path),
        "captionCount": len(caption_segments),
        "assetCount": len(asset_segments),
        "languageCode": getattr(transcript, "language_code", None),
    }
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(json.dumps({"error": str(error)}, ensure_ascii=False), file=sys.stderr)
        raise SystemExit(1)
