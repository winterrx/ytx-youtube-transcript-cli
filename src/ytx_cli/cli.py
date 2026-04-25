from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from typing import Iterable, List, Optional, Sequence
from urllib.parse import parse_qs, urlparse


VIDEO_ID_RE = re.compile(r"^[A-Za-z0-9_-]{11}$")


class CliError(Exception):
    """A user-facing error."""


class ClipboardError(CliError):
    """Raised when clipboard copying is unavailable."""


@dataclass(frozen=True)
class TranscriptInfo:
    language: str
    language_code: str
    is_generated: bool
    is_translatable: bool


def parse_video_id(value: str) -> str:
    candidate = value.strip()
    if VIDEO_ID_RE.match(candidate):
        return candidate

    parsed = urlparse(candidate)
    host = parsed.netloc.lower().removeprefix("www.")
    path_parts = [part for part in parsed.path.split("/") if part]

    video_id: Optional[str] = None
    if host in {"youtube.com", "m.youtube.com", "music.youtube.com"}:
        if parsed.path == "/watch":
            video_id = parse_qs(parsed.query).get("v", [None])[0]
        elif path_parts and path_parts[0] in {"shorts", "embed", "v", "live"}:
            video_id = path_parts[1] if len(path_parts) > 1 else None
    elif host in {"youtu.be", "youtube-nocookie.com"}:
        if host == "youtu.be":
            video_id = path_parts[0] if path_parts else None
        elif path_parts[:1] == ["embed"] and len(path_parts) > 1:
            video_id = path_parts[1]

    if video_id and VIDEO_ID_RE.match(video_id):
        return video_id

    raise CliError(f"Could not find a YouTube video ID in: {value}")


def format_timestamp(seconds: float) -> str:
    total_seconds = int(seconds)
    hours, remainder = divmod(total_seconds, 3600)
    minutes, secs = divmod(remainder, 60)
    if hours:
        return f"{hours}:{minutes:02d}:{secs:02d}"
    return f"{minutes}:{secs:02d}"


def format_transcript(snippets: Iterable[object], *, timestamps: bool = False) -> str:
    lines: List[str] = []
    for snippet in snippets:
        text = getattr(snippet, "text", "").strip()
        if not text:
            continue

        if timestamps:
            start = float(getattr(snippet, "start", 0.0))
            lines.append(f"[{format_timestamp(start)}] {text}")
        else:
            lines.append(text)

    return "\n".join(lines).strip()


def fetch_transcript(
    video_id: str,
    *,
    languages: Sequence[str],
    preserve_formatting: bool = False,
    allow_language_fallback: bool = True,
) -> object:
    from youtube_transcript_api import YouTubeTranscriptApi
    from youtube_transcript_api._errors import NoTranscriptFound

    api = YouTubeTranscriptApi()
    try:
        return api.fetch(
            video_id,
            languages=languages,
            preserve_formatting=preserve_formatting,
        )
    except NoTranscriptFound:
        if not allow_language_fallback:
            raise

        transcript_list = list(api.list(video_id))
        if not transcript_list:
            raise

        preferred = sorted(transcript_list, key=lambda item: item.is_generated)
        return preferred[0].fetch(preserve_formatting=preserve_formatting)


def list_transcripts(video_id: str) -> List[TranscriptInfo]:
    from youtube_transcript_api import YouTubeTranscriptApi

    api = YouTubeTranscriptApi()
    return [
        TranscriptInfo(
            language=transcript.language,
            language_code=transcript.language_code,
            is_generated=transcript.is_generated,
            is_translatable=transcript.is_translatable,
        )
        for transcript in api.list(video_id)
    ]


def copy_to_clipboard(text: str) -> None:
    command: Optional[Sequence[str]] = None

    if sys.platform == "darwin" and shutil.which("pbcopy"):
        command = ["pbcopy"]
    elif os.name == "nt" and shutil.which("clip"):
        command = ["clip"]
    elif shutil.which("wl-copy"):
        command = ["wl-copy"]
    elif shutil.which("xclip"):
        command = ["xclip", "-selection", "clipboard"]
    elif shutil.which("xsel"):
        command = ["xsel", "--clipboard", "--input"]

    if command is None:
        raise ClipboardError("No clipboard command found. Install pbcopy, wl-copy, xclip, or xsel.")

    subprocess.run(command, input=text.encode("utf-8"), check=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ytx",
        description="Print a YouTube video's transcript.",
    )
    parser.add_argument("video", help="YouTube URL or 11-character video ID")
    parser.add_argument(
        "-c",
        "--c",
        "--copy",
        action="store_true",
        dest="copy",
        help="copy the printed transcript to the clipboard",
    )
    parser.add_argument(
        "-l",
        "--lang",
        nargs="+",
        default=["en"],
        help="preferred transcript languages, in order (default: en)",
    )
    parser.add_argument(
        "--timestamps",
        action="store_true",
        help="prefix each transcript line with its start time",
    )
    parser.add_argument(
        "--preserve-formatting",
        action="store_true",
        help="keep YouTube's inline caption formatting where available",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list available transcripts instead of printing one",
    )
    return parser


def describe_error(exc: BaseException) -> str:
    try:
        from youtube_transcript_api._errors import (
            AgeRestricted,
            InvalidVideoId,
            IpBlocked,
            NoTranscriptFound,
            RequestBlocked,
            TranscriptsDisabled,
            VideoUnavailable,
        )
    except Exception:
        return str(exc)

    if isinstance(exc, InvalidVideoId):
        return "That does not look like a valid YouTube video ID."
    if isinstance(exc, VideoUnavailable):
        return "YouTube says this video is unavailable."
    if isinstance(exc, AgeRestricted):
        return "This video is age-restricted. Try authenticated cookies with the underlying API."
    if isinstance(exc, TranscriptsDisabled):
        return "Transcripts are disabled for this video."
    if isinstance(exc, NoTranscriptFound):
        return "No transcript was found for the requested language. Try `ytx --list <url>` or pass another `--lang`."
    if isinstance(exc, (RequestBlocked, IpBlocked)):
        return "YouTube blocked this request from the current IP. Try again later or use the proxy/cookie support in youtube-transcript-api."
    return str(exc)


def print_available_transcripts(video_id: str) -> None:
    transcripts = list_transcripts(video_id)
    if not transcripts:
        raise CliError("No transcripts are available for this video.")

    for transcript in transcripts:
        kind = "auto" if transcript.is_generated else "manual"
        translatable = "translatable" if transcript.is_translatable else "not translatable"
        print(f"{transcript.language_code}\t{kind}\t{translatable}\t{transcript.language}")


def run(args: argparse.Namespace) -> int:
    video_id = parse_video_id(args.video)

    if args.list:
        print_available_transcripts(video_id)
        return 0

    transcript = fetch_transcript(
        video_id,
        languages=args.lang,
        preserve_formatting=args.preserve_formatting,
        allow_language_fallback=args.lang == ["en"],
    )
    output = format_transcript(transcript, timestamps=args.timestamps)
    if not output:
        raise CliError("Transcript was found, but it did not contain printable text.")

    print(output)

    if args.copy:
        copy_to_clipboard(output)
        print("Copied transcript to clipboard.", file=sys.stderr)

    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        return run(args)
    except CliError as exc:
        print(f"ytx: {exc}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as exc:
        print(f"ytx: clipboard command failed: {exc}", file=sys.stderr)
        return 3
    except Exception as exc:
        print(f"ytx: {describe_error(exc)}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
