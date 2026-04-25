from types import SimpleNamespace

from ytx_cli.cli import format_timestamp, format_transcript, main, parse_video_id


def test_parse_video_id_accepts_plain_id():
    assert parse_video_id("dQw4w9WgXcQ") == "dQw4w9WgXcQ"


def test_parse_video_id_accepts_watch_url():
    assert (
        parse_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=abc")
        == "dQw4w9WgXcQ"
    )


def test_parse_video_id_accepts_short_url():
    assert parse_video_id("https://youtu.be/dQw4w9WgXcQ?si=abc") == "dQw4w9WgXcQ"


def test_parse_video_id_accepts_shorts_url():
    assert parse_video_id("https://youtube.com/shorts/dQw4w9WgXcQ") == "dQw4w9WgXcQ"


def test_format_timestamp():
    assert format_timestamp(65.9) == "1:05"
    assert format_timestamp(3661) == "1:01:01"


def test_format_transcript_plain_text():
    snippets = [
        SimpleNamespace(text=" hello ", start=0),
        SimpleNamespace(text="", start=1),
        SimpleNamespace(text="world", start=2),
    ]
    assert format_transcript(snippets) == "hello\nworld"


def test_format_transcript_with_timestamps():
    snippets = [SimpleNamespace(text="hello", start=65)]
    assert format_transcript(snippets, timestamps=True) == "[1:05] hello"


def test_main_prints_and_copies(monkeypatch, capsys):
    copied = {}

    def fake_fetch_transcript(*args, **kwargs):
        return [
            SimpleNamespace(text="hello", start=0),
            SimpleNamespace(text="world", start=1),
        ]

    def fake_copy_to_clipboard(text):
        copied["text"] = text

    monkeypatch.setattr("ytx_cli.cli.fetch_transcript", fake_fetch_transcript)
    monkeypatch.setattr("ytx_cli.cli.copy_to_clipboard", fake_copy_to_clipboard)

    assert main(["--c", "https://youtu.be/dQw4w9WgXcQ"]) == 0

    captured = capsys.readouterr()
    assert captured.out == "hello\nworld\n"
    assert "Copied transcript to clipboard." in captured.err
    assert copied["text"] == "hello\nworld"


def test_main_reports_bad_urls(capsys):
    assert main(["https://example.com/nope"]) == 2
    captured = capsys.readouterr()
    assert "Could not find a YouTube video ID" in captured.err
