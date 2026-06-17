#!/usr/bin/env python3
"""Exporta um projeto do FlowContent Auto como draft local do CapCut."""

from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from typing import Any
from uuid import uuid4


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Exporta um draft do CapCut.")
    parser.add_argument("--project-root", required=True, help="Pasta da produção local.")
    parser.add_argument("--capcut-root", help="Pasta com.lveditor.draft do CapCut.")
    return parser.parse_args()


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def uuid_str() -> str:
    return str(uuid4()).upper()


def now_micros() -> int:
    return int(time.time() * 1_000_000)


def forward_slashes(path: Path | str) -> str:
    return str(path).replace("\\", "/")


def detect_capcut_root(explicit_root: str | None) -> Path:
    if explicit_root:
        candidate = Path(explicit_root).expanduser()
        if candidate.is_dir():
            return candidate
        raise RuntimeError(f"Pasta do CapCut não encontrada: {candidate}")

    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        candidate = Path(local_app_data) / "CapCut" / "User Data" / "Projects" / "com.lveditor.draft"
        if candidate.is_dir():
            return candidate

    for root_meta in Path.home().rglob("root_meta_info.json"):
        if root_meta.parent.name == "com.lveditor.draft":
            return root_meta.parent

    raise RuntimeError("Não foi possível localizar a pasta Projects do CapCut neste computador.")


def clone_with_id(prototype: dict[str, Any]) -> dict[str, Any]:
    cloned = copy.deepcopy(prototype)
    cloned["id"] = uuid_str()
    return cloned


def material_index(materials: dict[str, Any]) -> dict[str, tuple[str, dict[str, Any]]]:
    indexed: dict[str, tuple[str, dict[str, Any]]] = {}
    for key, items in materials.items():
        if not isinstance(items, list):
            continue
        for item in items:
            if isinstance(item, dict) and "id" in item:
                indexed[item["id"]] = (key, item)
    return indexed


def load_ref_bundle(
    segment: dict[str, Any],
    indexed_materials: dict[str, tuple[str, dict[str, Any]]],
) -> dict[str, dict[str, Any]]:
    bundle: dict[str, dict[str, Any]] = {}
    for ref in segment.get("extra_material_refs", []):
        key, item = indexed_materials.get(ref, ("", None))
        if key and item:
            bundle[key] = item
    return bundle


def pick_caption_srt_path(project_root: Path, production: dict[str, Any]) -> Path | None:
    raw = production.get("captionSrtPath")
    if isinstance(raw, str) and raw:
        path = Path(raw)
        if path.is_file():
            return path
    for candidate in sorted((project_root / "srt").glob("*.legendas.srt")):
        return candidate
    return None


def pick_audio_path(project_root: Path, production: dict[str, Any]) -> Path | None:
    raw = production.get("audioPath")
    if isinstance(raw, str) and raw:
        path = Path(raw)
        if path.is_file():
            return path
    audio_dir = project_root / "audio"
    for candidate in sorted(audio_dir.iterdir()) if audio_dir.is_dir() else []:
        if candidate.is_file():
            return candidate
    return None


def project_title(project_root: Path, production: dict[str, Any]) -> str:
    title = production.get("title")
    if isinstance(title, str) and title.strip():
        return title.strip()
    return project_root.name


def scan_capcut_prototypes(capcut_root: Path) -> dict[str, Any]:
    root_meta_path = capcut_root / "root_meta_info.json"
    if not root_meta_path.is_file():
        raise RuntimeError("root_meta_info.json do CapCut não foi encontrado.")

    prototypes: dict[str, Any] = {
        "root_meta_path": root_meta_path,
        "root_meta": read_json(root_meta_path),
    }

    for draft_dir in sorted(path for path in capcut_root.iterdir() if path.is_dir() and not path.name.startswith(".")):
        content_path = draft_dir / "draft_content.json"
        meta_path = draft_dir / "draft_meta_info.json"
        if not content_path.is_file() or not meta_path.is_file():
            continue

        content = read_json(content_path)
        meta = read_json(meta_path)
        materials = content.get("materials", {})
        indexed = material_index(materials)
        tracks = {track.get("type"): track for track in content.get("tracks", [])}

        if "base_content" not in prototypes and {"video", "audio", "text"}.issubset(tracks):
            prototypes["base_content"] = content
            prototypes["base_meta"] = meta
            prototypes["base_dir"] = draft_dir
            prototypes["video_track_proto"] = tracks["video"]
            prototypes["audio_track_proto"] = tracks["audio"]
            prototypes["text_track_proto"] = tracks["text"]

        if "video_visual_proto" not in prototypes and "video" in tracks:
            for segment in tracks["video"].get("segments", []):
                _, material = indexed.get(segment.get("material_id", ""), ("", None))
                if material and material.get("type") == "video":
                    prototypes["video_visual_proto"] = {
                        "segment": segment,
                        "material": material,
                        "refs": load_ref_bundle(segment, indexed),
                    }
                    break

        if "photo_visual_proto" not in prototypes and "video" in tracks:
            for segment in tracks["video"].get("segments", []):
                _, material = indexed.get(segment.get("material_id", ""), ("", None))
                if material and material.get("type") == "photo":
                    prototypes["photo_visual_proto"] = {
                        "segment": segment,
                        "material": material,
                        "refs": load_ref_bundle(segment, indexed),
                    }
                    break

        if "audio_proto" not in prototypes and "audio" in tracks and materials.get("audios"):
            segment = tracks["audio"]["segments"][0]
            prototypes["audio_proto"] = {
                "segment": segment,
                "material": indexed[segment["material_id"]][1],
                "refs": load_ref_bundle(segment, indexed),
            }

        if "text_proto" not in prototypes and "text" in tracks and materials.get("texts"):
            segment = tracks["text"]["segments"][0]
            prototypes["text_proto"] = {
                "segment": segment,
                "material": indexed[segment["material_id"]][1],
                "refs": load_ref_bundle(segment, indexed),
            }

        for item_group in meta.get("draft_materials", []):
            if item_group.get("type") != 0:
                continue
            for item in item_group.get("value", []):
                metetype = item.get("metetype")
                if metetype == "video" and "video_meta_proto" not in prototypes:
                    prototypes["video_meta_proto"] = item
                if metetype == "photo" and "photo_meta_proto" not in prototypes:
                    prototypes["photo_meta_proto"] = item
                if metetype == "music" and "audio_meta_proto" not in prototypes:
                    prototypes["audio_meta_proto"] = item
        for item_group in meta.get("draft_materials", []):
            if item_group.get("type") == 2 and item_group.get("value") and "subtitle_meta_proto" not in prototypes:
                prototypes["subtitle_meta_proto"] = item_group["value"][0]

    required = [
        "base_content",
        "base_meta",
        "base_dir",
        "video_track_proto",
        "audio_track_proto",
        "text_track_proto",
        "video_visual_proto",
        "photo_visual_proto",
        "audio_proto",
        "text_proto",
        "video_meta_proto",
        "photo_meta_proto",
        "audio_meta_proto",
    ]
    missing = [key for key in required if key not in prototypes]
    if missing:
        raise RuntimeError(f"Não foi possível encontrar protótipos suficientes no CapCut: {', '.join(missing)}")
    return prototypes


def ffprobe_metadata(path: Path) -> tuple[int | None, int | None, int | None]:
    output = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,duration",
            "-of",
            "json",
            str(path),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    payload = json.loads(output.stdout or "{}")
    streams = payload.get("streams") or []
    if not streams:
        return None, None, None
    stream = streams[0]
    width = stream.get("width")
    height = stream.get("height")
    duration = stream.get("duration")
    duration_us = int(round(float(duration) * 1_000_000)) if duration not in (None, "") else None
    return duration_us, width, height


def locate_slot_media(project_root: Path, slot: dict[str, Any], source_order: int) -> Path:
    raw = slot.get("localPath")
    if isinstance(raw, str) and raw and Path(raw).is_file():
        return Path(raw)
    downloads_dir = project_root / "downloads"
    matches = sorted(downloads_dir.glob(f"{source_order:02d}.*")) + sorted(downloads_dir.glob(f"{source_order}.*"))
    for candidate in matches:
        if candidate.is_file():
            return candidate
    raise RuntimeError(f"Asset do slot {source_order:02d} não foi encontrado no projeto.")


def build_visual_assets(
    project_root: Path,
    manifest_assets: list[dict[str, Any]],
    generation_slots: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], int]:
    slot_index = {
        int(slot.get("sourceOrder")): slot
        for slot in generation_slots
        if slot.get("sourceOrder") is not None
    }
    visuals: list[dict[str, Any]] = []
    timeline_end = 0

    for asset in sorted(manifest_assets, key=lambda item: int(item["source_order"])):
        source_order = int(asset["source_order"])
        slot = slot_index.get(source_order)
        if not slot:
            raise RuntimeError(f"Slot {source_order:02d} não está disponível para exportação do CapCut.")
        media_path = locate_slot_media(project_root, slot, source_order)
        target_start_us = int(asset["start"]) * 1_000
        target_duration_us = max(1_000, (int(asset["end"]) - int(asset["start"])) * 1_000)
        media_duration_us, width, height = ffprobe_metadata(media_path)
        suffix = media_path.suffix.lower()
        is_photo = suffix in {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif"}

        if is_photo:
            source_start_us = 0
            source_duration_us = target_duration_us
            media_duration_us = media_duration_us or 10_800_000_000
        else:
            if not media_duration_us:
                raise RuntimeError(f"Não foi possível ler a duração do vídeo {media_path.name}.")
            source_duration_us = min(target_duration_us, media_duration_us)
            source_start_us = max(0, (media_duration_us - source_duration_us) // 2)

        visuals.append(
            {
                "source_order": source_order,
                "path": media_path,
                "file_name": media_path.name,
                "target_start_us": target_start_us,
                "target_duration_us": target_duration_us,
                "source_start_us": source_start_us,
                "source_duration_us": source_duration_us,
                "media_duration_us": media_duration_us,
                "width": width or 1280,
                "height": height or 720,
                "kind": "photo" if is_photo else "video",
            }
        )
        timeline_end = max(timeline_end, target_start_us + target_duration_us)

    return visuals, timeline_end


def sync_text_content(material: dict[str, Any], text: str) -> None:
    content = material.get("content")
    if not isinstance(content, str) or not content:
        return
    try:
        payload = json.loads(content)
    except json.JSONDecodeError:
        return
    payload["text"] = text
    styles = payload.get("styles")
    if isinstance(styles, list):
        for style in styles:
            if isinstance(style, dict):
                style["range"] = [0, len(text)]
    material["content"] = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))


def copy_template_files(template_dir: Path, target_dir: Path) -> None:
    target_dir.mkdir(parents=True, exist_ok=True)
    for item in template_dir.iterdir():
        destination = target_dir / item.name
        if item.is_dir():
            shutil.copytree(item, destination, dirs_exist_ok=True)
        else:
            shutil.copy2(item, destination)


def build_canvas(project_settings: dict[str, Any], base_canvas: dict[str, Any]) -> dict[str, Any]:
    canvas = copy.deepcopy(base_canvas)
    ratio = (
        project_settings.get("videoAspectRatio")
        or project_settings.get("imageAspectRatio")
        or "VIDEO_ASPECT_RATIO_LANDSCAPE"
    )
    if ratio in {"VIDEO_ASPECT_RATIO_PORTRAIT", "IMAGE_ASPECT_RATIO_PORTRAIT"}:
        canvas["width"] = 1080
        canvas["height"] = 1920
        canvas["ratio"] = "9:16"
    elif ratio in {"VIDEO_ASPECT_RATIO_SQUARE", "IMAGE_ASPECT_RATIO_SQUARE"}:
        canvas["width"] = 1080
        canvas["height"] = 1080
        canvas["ratio"] = "1:1"
    else:
        canvas["width"] = 1920
        canvas["height"] = 1080
        canvas["ratio"] = "16:9"
    return canvas


def build_draft(
    project_root: Path,
    capcut_root: Path,
    prototypes: dict[str, Any],
) -> dict[str, Any]:
    production = read_json(project_root / ".flowcontent" / "production.json")
    manifest = read_json(project_root / ".flowcontent" / "audio-segments.json")
    manifest_assets = manifest.get("assets") or []
    manifest_captions = manifest.get("captions") or []
    if not manifest_assets:
        raise RuntimeError("O projeto não possui segmentos de assets para exportar ao CapCut.")

    visuals, visual_timeline_end = build_visual_assets(
        project_root,
        manifest_assets,
        production.get("generationSlots") or [],
    )
    title = project_title(project_root, production)
    caption_srt_path = pick_caption_srt_path(project_root, production)
    audio_path = pick_audio_path(project_root, production)
    now_us = now_micros()
    draft_folder_name = f"flowcontent-{int(time.time())}"
    draft_dir = capcut_root / draft_folder_name
    draft_id = uuid_str()
    copy_template_files(prototypes["base_dir"], draft_dir)

    base_content = copy.deepcopy(prototypes["base_content"])
    base_content["id"] = draft_id
    base_content["name"] = title
    base_content["path"] = forward_slashes(draft_dir)
    base_content["duration"] = visual_timeline_end
    base_content["update_time"] = now_us
    base_content["create_time"] = now_us
    base_content["canvas_config"] = build_canvas(production.get("settings") or {}, base_content.get("canvas_config") or {})

    materials = base_content.get("materials") or {}
    for key, value in list(materials.items()):
        if isinstance(value, list):
            materials[key] = []
    base_content["materials"] = materials

    video_track = copy.deepcopy(prototypes["video_track_proto"])
    video_track["id"] = uuid_str()
    video_track["segments"] = []
    text_track = copy.deepcopy(prototypes["text_track_proto"])
    text_track["id"] = uuid_str()
    text_track["segments"] = []
    audio_track = copy.deepcopy(prototypes["audio_track_proto"])
    audio_track["id"] = uuid_str()
    audio_track["segments"] = []

    meta_visuals: list[dict[str, Any]] = []
    timeline_materials_size = 0

    for render_index, visual in enumerate(visuals):
        proto = prototypes["photo_visual_proto"] if visual["kind"] == "photo" else prototypes["video_visual_proto"]
        material = clone_with_id(proto["material"])
        segment = clone_with_id(proto["segment"])
        speed = clone_with_id(proto["refs"]["speeds"])
        placeholder = clone_with_id(proto["refs"]["placeholder_infos"])
        canvas = clone_with_id(proto["refs"]["canvases"])
        sound_map = clone_with_id(proto["refs"]["sound_channel_mappings"])
        material_color = clone_with_id(proto["refs"]["material_colors"])
        vocal = clone_with_id(proto["refs"]["vocal_separations"])
        extra_refs = [speed["id"], placeholder["id"], canvas["id"], sound_map["id"], material_color["id"], vocal["id"]]

        if visual["kind"] == "video" and "material_animations" in proto["refs"]:
            animation = clone_with_id(proto["refs"]["material_animations"])
            materials["material_animations"].append(animation)
            extra_refs.insert(3, animation["id"])

        local_material_id = uuid_str()
        material["path"] = forward_slashes(visual["path"])
        material["material_name"] = visual["file_name"]
        material["duration"] = visual["media_duration_us"]
        material["has_audio"] = visual["kind"] == "video"
        material["width"] = visual["width"]
        material["height"] = visual["height"]
        material["type"] = visual["kind"]
        material["local_material_id"] = local_material_id
        material["category_name"] = "local"
        material["picture_from"] = "none"
        if visual["kind"] == "video":
            material["video_algorithm"]["time_range"] = {
                "start": 0,
                "duration": visual["media_duration_us"],
            }
        else:
            material["video_algorithm"]["time_range"] = None

        segment["material_id"] = material["id"]
        segment["source_timerange"] = {
            "start": visual["source_start_us"],
            "duration": visual["source_duration_us"],
        }
        segment["target_timerange"] = {
            "start": visual["target_start_us"],
            "duration": visual["target_duration_us"],
        }
        segment["render_timerange"] = {"start": 0, "duration": 0}
        segment["extra_material_refs"] = extra_refs
        segment["render_index"] = render_index
        segment["track_render_index"] = 0
        segment["volume"] = 0.0 if visual["kind"] == "photo" else segment.get("volume", 1.0)
        segment["last_nonzero_volume"] = 0.0 if visual["kind"] == "photo" else segment.get("last_nonzero_volume", 1.0)

        materials["videos"].append(material)
        materials["speeds"].append(speed)
        materials["placeholder_infos"].append(placeholder)
        materials["canvases"].append(canvas)
        materials["sound_channel_mappings"].append(sound_map)
        materials["material_colors"].append(material_color)
        materials["vocal_separations"].append(vocal)
        video_track["segments"].append(segment)

        meta_proto = prototypes["photo_meta_proto"] if visual["kind"] == "photo" else prototypes["video_meta_proto"]
        meta_item = copy.deepcopy(meta_proto)
        meta_item["id"] = local_material_id
        meta_item["file_Path"] = forward_slashes(visual["path"])
        meta_item["extra_info"] = visual["file_name"]
        meta_item["duration"] = visual["target_duration_us"]
        meta_item["width"] = visual["width"]
        meta_item["height"] = visual["height"]
        meta_item["create_time"] = int(time.time())
        meta_item["import_time"] = int(time.time())
        meta_item["import_time_ms"] = now_us + render_index
        meta_item["metetype"] = "photo" if visual["kind"] == "photo" else "video"
        if visual["kind"] == "photo":
            meta_item["roughcut_time_range"] = {"start": -1, "duration": -1}
        else:
            meta_item["roughcut_time_range"] = {"start": 0, "duration": visual["media_duration_us"]}
        meta_item["sub_time_range"] = {"start": -1, "duration": -1}
        meta_visuals.append(meta_item)
        timeline_materials_size += visual["path"].stat().st_size

    if audio_path:
        audio_duration_us, _, _ = ffprobe_metadata(audio_path)
        if not audio_duration_us:
            audio_duration_us = visual_timeline_end
        audio_material = clone_with_id(prototypes["audio_proto"]["material"])
        audio_segment = clone_with_id(prototypes["audio_proto"]["segment"])
        audio_speed = clone_with_id(prototypes["audio_proto"]["refs"]["speeds"])
        audio_placeholder = clone_with_id(prototypes["audio_proto"]["refs"]["placeholder_infos"])
        audio_beats = clone_with_id(prototypes["audio_proto"]["refs"]["beats"])
        audio_sound_map = clone_with_id(prototypes["audio_proto"]["refs"]["sound_channel_mappings"])
        audio_vocal = clone_with_id(prototypes["audio_proto"]["refs"]["vocal_separations"])
        local_audio_id = uuid_str()

        audio_material["path"] = forward_slashes(audio_path)
        audio_material["name"] = audio_path.name
        audio_material["duration"] = audio_duration_us
        audio_material["local_material_id"] = local_audio_id
        audio_segment["material_id"] = audio_material["id"]
        audio_segment["source_timerange"] = {"start": 0, "duration": audio_duration_us}
        audio_segment["target_timerange"] = {"start": 0, "duration": audio_duration_us}
        audio_segment["render_timerange"] = {"start": 0, "duration": 0}
        audio_segment["extra_material_refs"] = [
            audio_speed["id"],
            audio_placeholder["id"],
            audio_beats["id"],
            audio_sound_map["id"],
            audio_vocal["id"],
        ]
        audio_segment["track_render_index"] = 2

        materials["audios"].append(audio_material)
        materials["speeds"].append(audio_speed)
        materials["placeholder_infos"].append(audio_placeholder)
        materials["beats"].append(audio_beats)
        materials["sound_channel_mappings"].append(audio_sound_map)
        materials["vocal_separations"].append(audio_vocal)
        audio_track["segments"].append(audio_segment)

        audio_meta = copy.deepcopy(prototypes["audio_meta_proto"])
        audio_meta["id"] = local_audio_id
        audio_meta["file_Path"] = forward_slashes(audio_path)
        audio_meta["extra_info"] = audio_path.name
        audio_meta["duration"] = audio_duration_us
        audio_meta["create_time"] = int(time.time())
        audio_meta["import_time"] = int(time.time())
        audio_meta["import_time_ms"] = now_us
        audio_meta["roughcut_time_range"] = {"start": 0, "duration": audio_duration_us}
        meta_visuals.insert(0, audio_meta)
        timeline_materials_size += audio_path.stat().st_size
        visual_timeline_end = max(visual_timeline_end, audio_duration_us)

    import_group_id = f"flowcontent_{int(time.time())}"
    for index, caption in enumerate(manifest_captions):
        text_material = clone_with_id(prototypes["text_proto"]["material"])
        text_segment = clone_with_id(prototypes["text_proto"]["segment"])
        text_animation = clone_with_id(prototypes["text_proto"]["refs"]["material_animations"])
        text = str(caption.get("text", "")).strip()
        if not text:
            continue

        text_material["group_id"] = import_group_id
        sync_text_content(text_material, text)
        text_segment["material_id"] = text_material["id"]
        text_segment["target_timerange"] = {
            "start": int(caption["start"]) * 1_000,
            "duration": max(1_000, (int(caption["end"]) - int(caption["start"])) * 1_000),
        }
        text_segment["source_timerange"] = None
        text_segment["extra_material_refs"] = [text_animation["id"]]
        text_segment["render_index"] = 14_000 + index
        text_segment["track_render_index"] = 1
        materials["texts"].append(text_material)
        materials["material_animations"].append(text_animation)
        text_track["segments"].append(text_segment)

    base_content["tracks"] = [video_track, text_track, audio_track]
    base_content["duration"] = visual_timeline_end

    base_meta = copy.deepcopy(prototypes["base_meta"])
    base_meta["draft_id"] = draft_id
    base_meta["draft_name"] = title
    base_meta["draft_fold_path"] = forward_slashes(draft_dir)
    base_meta["draft_root_path"] = forward_slashes(capcut_root)
    base_meta["draft_cover"] = str(draft_dir / "draft_cover.jpg").replace("\\", "/")
    base_meta["tm_duration"] = visual_timeline_end
    base_meta["tm_draft_create"] = now_us
    base_meta["tm_draft_modified"] = now_us
    base_meta["draft_timeline_materials_size_"] = timeline_materials_size

    draft_materials: list[dict[str, Any]] = []
    for item in base_meta.get("draft_materials", []):
        cloned = copy.deepcopy(item)
        if cloned.get("type") == 0:
            cloned["value"] = meta_visuals
        elif cloned.get("type") == 2:
            if caption_srt_path and "subtitle_meta_proto" in prototypes:
                subtitle_meta = copy.deepcopy(prototypes["subtitle_meta_proto"])
                subtitle_meta["id"] = uuid_str()
                subtitle_meta["file_Path"] = forward_slashes(caption_srt_path)
                subtitle_meta["extra_info"] = caption_srt_path.name
                cloned["value"] = [subtitle_meta]
            else:
                cloned["value"] = []
        else:
            cloned["value"] = []
        draft_materials.append(cloned)
    base_meta["draft_materials"] = draft_materials

    root_meta_path = prototypes["root_meta_path"]
    root_meta = prototypes["root_meta"]
    base_root_entry = copy.deepcopy(root_meta["all_draft_store"][0]) if root_meta.get("all_draft_store") else {}
    base_root_entry["draft_id"] = draft_id
    base_root_entry["draft_name"] = title
    base_root_entry["draft_fold_path"] = forward_slashes(draft_dir)
    base_root_entry["draft_json_file"] = forward_slashes(draft_dir / "draft_content.json")
    base_root_entry["draft_cover"] = forward_slashes(draft_dir / "draft_cover.jpg")
    base_root_entry["draft_root_path"] = forward_slashes(capcut_root)
    base_root_entry["draft_timeline_materials_size"] = timeline_materials_size
    base_root_entry["tm_duration"] = visual_timeline_end
    base_root_entry["tm_draft_create"] = now_us
    base_root_entry["tm_draft_modified"] = now_us
    root_meta.setdefault("all_draft_store", []).insert(0, base_root_entry)
    root_meta["draft_ids"] = int(root_meta.get("draft_ids", 0)) + 1
    root_meta["root_path"] = forward_slashes(capcut_root)

    write_json(draft_dir / "draft_content.json", base_content)
    write_json(draft_dir / "draft_content.json.bak", base_content)
    write_json(draft_dir / "draft_meta_info.json", base_meta)
    write_json(root_meta_path, root_meta)

    return {
        "capcutRoot": forward_slashes(capcut_root),
        "draftPath": forward_slashes(draft_dir),
        "draftName": title,
        "draftId": draft_id,
        "durationUs": visual_timeline_end,
    }


def main() -> int:
    args = parse_args()
    project_root = Path(args.project_root).resolve()
    if not project_root.is_dir():
        raise RuntimeError(f"Pasta do projeto não encontrada: {project_root}")

    capcut_root = detect_capcut_root(args.capcut_root)
    prototypes = scan_capcut_prototypes(capcut_root)
    result = build_draft(project_root, capcut_root, prototypes)
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(json.dumps({"error": str(error)}, ensure_ascii=False), file=sys.stderr)
        raise SystemExit(1)
