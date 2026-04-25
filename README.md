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
cargo install --git https://github.com/winterrx/ytx-youtube-transcript-cli.git --force
```

From this directory:

```sh
cargo install --path . --force
```

## Usage

```sh
ytx <youtube-url-or-video-id>
ytx --lang en es <youtube-url-or-video-id>
ytx --timestamps <youtube-url-or-video-id>
ytx --list <youtube-url-or-video-id>
```

This is a Rust CLI. It works for videos where YouTube exposes captions or
auto-captions. Private videos, videos with captions disabled, protected videos,
or temporary YouTube/IP blocks can still fail.
