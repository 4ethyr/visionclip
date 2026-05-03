#!/usr/bin/env python3
import argparse
import os
import sys
import wave
from pathlib import Path

from faster_whisper import WhisperModel


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Transcribe a WAV file with faster-whisper.")
    parser.add_argument("wav_path")
    parser.add_argument("--model", default=os.environ.get("VISIONCLIP_STT_MODEL", "base"))
    parser.add_argument("--language", default=os.environ.get("VISIONCLIP_STT_LANGUAGE", "auto"))
    parser.add_argument(
        "--cache-dir",
        default=os.environ.get(
            "VISIONCLIP_STT_CACHE",
            str(Path(__file__).resolve().parents[1] / "stt-cache"),
        ),
    )
    parser.add_argument("--device", default=os.environ.get("VISIONCLIP_STT_DEVICE", "cpu"))
    parser.add_argument(
        "--compute-type",
        default=os.environ.get("VISIONCLIP_STT_COMPUTE_TYPE", "int8"),
    )
    parser.add_argument(
        "--beam-size",
        type=int,
        default=int(os.environ.get("VISIONCLIP_STT_BEAM_SIZE", "5")),
    )
    parser.add_argument(
        "--vad-filter",
        default=os.environ.get("VISIONCLIP_STT_VAD_FILTER", "true"),
        help="Use true/false. VAD helps remove silence/noise around short commands.",
    )
    return parser.parse_args()


def optional_language(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.strip().lower()
    if normalized in {"", "auto", "detect", "none", "multilingual"}:
        return None
    return normalized


def parse_bool(value: str) -> bool:
    return value.strip().lower() not in {"0", "false", "no", "off"}


def main() -> int:
    args = parse_args()
    wav_path = Path(args.wav_path)
    if not wav_path.exists():
        print(f"audio file not found: {wav_path}", file=sys.stderr)
        return 2

    try:
        with wave.open(str(wav_path), "rb") as wav:
            frames = wav.getnframes()
            rate = wav.getframerate()
            channels = wav.getnchannels()
            duration = frames / rate if rate else 0.0
            print(
                f"audio={wav_path} bytes={wav_path.stat().st_size} duration={duration:.2f}s rate={rate} channels={channels}",
                file=sys.stderr,
            )
    except wave.Error as error:
        print(f"failed to inspect WAV header: {error}", file=sys.stderr)

    cache_dir = Path(args.cache_dir)
    cache_dir.mkdir(parents=True, exist_ok=True)

    model = WhisperModel(
        args.model,
        device=args.device,
        compute_type=args.compute_type,
        download_root=str(cache_dir),
    )
    language = optional_language(args.language)
    vad_filter = parse_bool(args.vad_filter)
    segments, info = model.transcribe(
        args.wav_path,
        language=language,
        beam_size=max(args.beam_size, 1),
        vad_filter=vad_filter,
    )
    detected_language = getattr(info, "language", None)
    if detected_language:
        print(f"detected_language={detected_language}", file=sys.stderr)
    transcript = " ".join(segment.text.strip() for segment in segments).strip()
    if transcript:
        print(transcript)
    else:
        print("no speech recognized", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
