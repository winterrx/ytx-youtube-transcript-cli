# ytx

Tiny YouTube transcript CLI.

```sh
ytx "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
ytx --c "https://youtu.be/dQw4w9WgXcQ"
```

`--c` copies the same transcript to your clipboard after printing it. `-c` and
`--copy` work too.

## Install

From GitHub:

```sh
uv tool install git+https://github.com/winterrx/ytx-youtube-transcript-cli.git
```

From this directory:

```sh
uv tool install .
```

Or, inside a virtual environment:

```sh
python -m pip install .
```

## Usage

```sh
ytx <youtube-url-or-video-id>
ytx --lang en es <youtube-url-or-video-id>
ytx --timestamps <youtube-url-or-video-id>
ytx --list <youtube-url-or-video-id>
```

This uses `youtube-transcript-api`, so it works for videos where YouTube exposes
captions or auto-captions. Private videos, videos with captions disabled, or
temporary YouTube/IP blocks can still fail.
