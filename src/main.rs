use std::env;
use std::fmt;
use std::io::Write;
use std::process::{Command, Stdio};

use url::Url;
use ytt::{TranscriptError, TranscriptInfo, TranscriptItem, YouTubeTranscript};

const HELP: &str = "\
usage: ytx [-h] [-c] [-l LANG [LANG ...]] [--timestamps] [--preserve-formatting] [--list] video

Print a YouTube video's transcript.

positional arguments:
  video                 YouTube URL or 11-character video ID

options:
  -h, --help            show this help message and exit
  -c, --c, --copy       copy the printed transcript to the clipboard
  -l, --lang LANG [LANG ...]
                        preferred transcript languages, in order (default: en)
  --timestamps          prefix each transcript line with its start time
  --preserve-formatting
                        accepted for compatibility with the Python version
  --list                list available transcripts instead of printing one
";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Config {
    video: String,
    copy: bool,
    languages: Vec<String>,
    timestamps: bool,
    preserve_formatting: bool,
    list: bool,
}

#[derive(Debug)]
enum CliError {
    Help,
    Usage(String),
    Clipboard(String),
    Transcript(TranscriptError),
}

impl CliError {
    fn exit_code(&self) -> i32 {
        match self {
            CliError::Help => 0,
            CliError::Usage(_) => 2,
            CliError::Clipboard(_) => 3,
            CliError::Transcript(_) => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Help => write!(f, "{HELP}"),
            CliError::Usage(message) => write!(f, "{message}"),
            CliError::Clipboard(message) => write!(f, "{message}"),
            CliError::Transcript(error) => write!(f, "{}", describe_transcript_error(error)),
        }
    }
}

impl From<TranscriptError> for CliError {
    fn from(error: TranscriptError) -> Self {
        CliError::Transcript(error)
    }
}

#[tokio::main]
async fn main() {
    let code = match run_from_env().await {
        Ok(()) => 0,
        Err(CliError::Help) => {
            print!("{HELP}");
            0
        }
        Err(error) => {
            eprintln!("ytx: {error}");
            error.exit_code()
        }
    };

    std::process::exit(code);
}

async fn run_from_env() -> Result<(), CliError> {
    let config = parse_args(env::args().skip(1))?;
    run(config).await
}

async fn run(config: Config) -> Result<(), CliError> {
    let video_id = parse_video_id(&config.video)?;
    let api = YouTubeTranscript::new();
    let _ = config.preserve_formatting;

    if config.list {
        print_available_transcripts(&api, &video_id).await?;
        return Ok(());
    }

    let transcript = fetch_transcript(&api, &video_id, &config.languages).await?;
    let output = format_transcript(&transcript.transcript, config.timestamps);
    if output.is_empty() {
        return Err(CliError::Usage(
            "Transcript was found, but it did not contain printable text.".to_string(),
        ));
    }

    println!("{output}");

    if config.copy {
        copy_to_clipboard(&output)?;
        eprintln!("Copied transcript to clipboard.");
    }

    Ok(())
}

fn parse_args<I>(args: I) -> Result<Config, CliError>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let mut video: Option<String> = None;
    let mut copy = false;
    let mut languages: Option<Vec<String>> = None;
    let mut timestamps = false;
    let mut preserve_formatting = false;
    let mut list = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Err(CliError::Help),
            "-c" | "--c" | "--copy" => {
                copy = true;
                i += 1;
            }
            "--timestamps" => {
                timestamps = true;
                i += 1;
            }
            "--preserve-formatting" => {
                preserve_formatting = true;
                i += 1;
            }
            "--list" => {
                list = true;
                i += 1;
            }
            "-l" | "--lang" => {
                i += 1;
                let values = collect_language_values(&args, &mut i, video.is_some());
                if values.is_empty() {
                    return Err(CliError::Usage(format!(
                        "expected at least one language after {}",
                        args[i.saturating_sub(1)]
                    )));
                }
                languages = Some(values);
            }
            value if value.starts_with("--lang=") => {
                let raw = value.trim_start_matches("--lang=");
                let values = split_language_values(raw);
                if values.is_empty() {
                    return Err(CliError::Usage(
                        "expected at least one language after --lang=".to_string(),
                    ));
                }
                languages = Some(values);
                i += 1;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Usage(format!("unknown option: {value}")));
            }
            value => {
                if video.is_some() {
                    return Err(CliError::Usage(format!("unexpected argument: {value}")));
                }
                video = Some(value.to_string());
                i += 1;
            }
        }
    }

    let Some(video) = video else {
        return Err(CliError::Usage(
            "missing required argument: video".to_string(),
        ));
    };

    Ok(Config {
        video,
        copy,
        languages: languages.unwrap_or_else(|| vec!["en".to_string()]),
        timestamps,
        preserve_formatting,
        list,
    })
}

fn collect_language_values(
    args: &[String],
    index: &mut usize,
    video_already_seen: bool,
) -> Vec<String> {
    let mut values = Vec::new();

    while *index < args.len() {
        let value = &args[*index];
        if value.starts_with('-') {
            break;
        }

        let should_treat_as_language =
            video_already_seen || has_later_non_option(args, *index + 1) || values.is_empty();

        if !should_treat_as_language {
            break;
        }

        values.extend(split_language_values(value));
        *index += 1;
    }

    values
}

fn has_later_non_option(args: &[String], start: usize) -> bool {
    args.iter().skip(start).any(|arg| !arg.starts_with('-'))
}

fn split_language_values(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_video_id(value: &str) -> Result<String, CliError> {
    let candidate = value.trim();
    if is_video_id(candidate) {
        return Ok(candidate.to_string());
    }

    let parsed = Url::parse(candidate)
        .map_err(|_| CliError::Usage(format!("Could not find a YouTube video ID in: {value}")))?;
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let path_parts: Vec<&str> = parsed
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();

    let video_id = match host.as_str() {
        "youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            if parsed.path() == "/watch" {
                parsed
                    .query_pairs()
                    .find(|(key, _)| key == "v")
                    .map(|(_, value)| value.into_owned())
            } else if matches!(
                path_parts.first(),
                Some(&"shorts" | &"embed" | &"v" | &"live")
            ) {
                path_parts.get(1).map(|id| id.to_string())
            } else {
                None
            }
        }
        "youtu.be" => path_parts.first().map(|id| id.to_string()),
        "youtube-nocookie.com" => {
            if path_parts.first() == Some(&"embed") {
                path_parts.get(1).map(|id| id.to_string())
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(video_id) = video_id {
        if is_video_id(&video_id) {
            return Ok(video_id);
        }
    }

    Err(CliError::Usage(format!(
        "Could not find a YouTube video ID in: {value}"
    )))
}

fn is_video_id(value: &str) -> bool {
    value.len() == 11
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

async fn print_available_transcripts(
    api: &YouTubeTranscript,
    video_id: &str,
) -> Result<(), CliError> {
    let transcript_list = api.list_transcripts(video_id).await?;
    let mut transcripts: Vec<&TranscriptInfo> = transcript_list.all_transcripts();
    transcripts.sort_by(|left, right| {
        left.is_generated
            .cmp(&right.is_generated)
            .then_with(|| left.language_code.cmp(&right.language_code))
    });

    if transcripts.is_empty() {
        return Err(CliError::Usage(
            "No transcripts are available for this video.".to_string(),
        ));
    }

    for transcript in transcripts {
        let kind = if transcript.is_generated {
            "auto"
        } else {
            "manual"
        };
        let translatable = if transcript.is_translatable {
            "translatable"
        } else {
            "not translatable"
        };
        println!(
            "{}\t{}\t{}\t{}",
            transcript.language_code, kind, translatable, transcript.language
        );
    }

    Ok(())
}

async fn fetch_transcript(
    api: &YouTubeTranscript,
    video_id: &str,
    languages: &[String],
) -> Result<ytt::TranscriptResponse, CliError> {
    let language_refs: Vec<&str> = languages.iter().map(String::as_str).collect();

    match api.fetch_transcript(video_id, Some(language_refs)).await {
        Ok(transcript) => Ok(transcript),
        Err(TranscriptError::NoTranscriptFound(_, _)) if languages == ["en"] => {
            let transcript_list = api.list_transcripts(video_id).await?;
            let Some(language_code) = fallback_language_code(&transcript_list.all_transcripts())
            else {
                return Err(TranscriptError::NoTranscriptFound(
                    video_id.to_string(),
                    languages.to_vec(),
                )
                .into());
            };
            api.fetch_transcript(video_id, Some(vec![language_code.as_str()]))
                .await
                .map_err(Into::into)
        }
        Err(TranscriptError::NoTranscriptFound(_, _)) => {
            let transcript_list = api.list_transcripts(video_id).await?;
            if let Some(language_code) =
                requested_language_code(&transcript_list.all_transcripts(), languages)
            {
                return api
                    .fetch_transcript(video_id, Some(vec![language_code.as_str()]))
                    .await
                    .map_err(Into::into);
            }
            Err(TranscriptError::NoTranscriptFound(video_id.to_string(), languages.to_vec()).into())
        }
        Err(error) => Err(error.into()),
    }
}

fn requested_language_code(
    transcripts: &[&TranscriptInfo],
    languages: &[String],
) -> Option<String> {
    for language in languages {
        if let Some(transcript) = find_language(transcripts, language, false) {
            return Some(transcript.language_code.clone());
        }
        if let Some(transcript) = find_language(transcripts, language, true) {
            return Some(transcript.language_code.clone());
        }
    }
    None
}

fn fallback_language_code(transcripts: &[&TranscriptInfo]) -> Option<String> {
    transcripts
        .iter()
        .min_by_key(|transcript| transcript.is_generated)
        .map(|transcript| transcript.language_code.clone())
}

fn find_language<'a>(
    transcripts: &'a [&'a TranscriptInfo],
    language: &str,
    allow_prefix: bool,
) -> Option<&'a TranscriptInfo> {
    transcripts
        .iter()
        .copied()
        .filter(|transcript| {
            transcript.language_code == language
                || (allow_prefix
                    && transcript
                        .language_code
                        .strip_prefix(language)
                        .is_some_and(|rest| rest.starts_with('-')))
        })
        .min_by_key(|transcript| transcript.is_generated)
}

fn format_transcript(snippets: &[TranscriptItem], timestamps: bool) -> String {
    snippets
        .iter()
        .filter_map(|snippet| {
            let text = snippet.text.trim();
            if text.is_empty() {
                return None;
            }
            if timestamps {
                Some(format!("[{}] {text}", format_timestamp(snippet.start)))
            } else {
                Some(text.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_timestamp(seconds: f64) -> String {
    let total_seconds = seconds as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), CliError> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };

    for (command, args) in commands {
        match run_clipboard_command(command, args, text) {
            Ok(()) => return Ok(()),
            Err(ClipboardCommandError::NotFound) => continue,
            Err(ClipboardCommandError::Failed(message)) => {
                return Err(CliError::Clipboard(message))
            }
        }
    }

    Err(CliError::Clipboard(
        "No clipboard command found. Install pbcopy, wl-copy, xclip, or xsel.".to_string(),
    ))
}

enum ClipboardCommandError {
    NotFound,
    Failed(String),
}

fn run_clipboard_command(
    command: &str,
    args: &[&str],
    text: &str,
) -> Result<(), ClipboardCommandError> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ClipboardCommandError::NotFound
            } else {
                ClipboardCommandError::Failed(format!(
                    "failed to start clipboard command `{command}`: {error}"
                ))
            }
        })?;

    let Some(stdin) = child.stdin.as_mut() else {
        return Err(ClipboardCommandError::Failed(format!(
            "clipboard command `{command}` did not open stdin"
        )));
    };
    stdin.write_all(text.as_bytes()).map_err(|error| {
        ClipboardCommandError::Failed(format!(
            "failed to write to clipboard command `{command}`: {error}"
        ))
    })?;
    drop(child.stdin.take());

    let status = child.wait().map_err(|error| {
        ClipboardCommandError::Failed(format!(
            "failed to wait for clipboard command `{command}`: {error}"
        ))
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(ClipboardCommandError::Failed(format!(
            "clipboard command `{command}` exited with {status}"
        )))
    }
}

fn describe_transcript_error(error: &TranscriptError) -> String {
    match error {
        TranscriptError::InvalidVideoId(_) => {
            "That does not look like a valid YouTube video ID.".to_string()
        }
        TranscriptError::VideoUnavailable(_) => "YouTube says this video is unavailable.".to_string(),
        TranscriptError::AgeRestricted(_) => {
            "This video is age-restricted.".to_string()
        }
        TranscriptError::TranscriptsDisabled(_) => {
            "Transcripts are disabled for this video.".to_string()
        }
        TranscriptError::NoTranscriptFound(_, _) => {
            "No transcript was found for the requested language. Try `ytx --list <url>` or pass another `--lang`.".to_string()
        }
        TranscriptError::RequestBlocked(_) | TranscriptError::IpBlocked(_) => {
            "YouTube blocked this request from the current IP. Try again later.".to_string()
        }
        TranscriptError::PoTokenRequired(_) => {
            "This protected video requires a YouTube token that ytx cannot fetch.".to_string()
        }
        _ => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_video_id_accepts_plain_id() {
        assert_eq!(parse_video_id("dQw4w9WgXcQ").unwrap(), "dQw4w9WgXcQ");
    }

    #[test]
    fn parse_video_id_accepts_watch_url() {
        assert_eq!(
            parse_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=abc").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn parse_video_id_accepts_short_url() {
        assert_eq!(
            parse_video_id("https://youtu.be/dQw4w9WgXcQ?si=abc").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn parse_video_id_accepts_shorts_url() {
        assert_eq!(
            parse_video_id("https://youtube.com/shorts/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn parse_video_id_rejects_non_youtube_url() {
        assert!(matches!(
            parse_video_id("https://example.com/nope"),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn parse_args_accepts_copy_after_video() {
        let config = parse_args(["https://youtu.be/dQw4w9WgXcQ".into(), "--c".into()]).unwrap();
        assert_eq!(config.video, "https://youtu.be/dQw4w9WgXcQ");
        assert!(config.copy);
    }

    #[test]
    fn parse_args_accepts_copy_before_video() {
        let config = parse_args(["--c".into(), "https://youtu.be/dQw4w9WgXcQ".into()]).unwrap();
        assert_eq!(config.video, "https://youtu.be/dQw4w9WgXcQ");
        assert!(config.copy);
    }

    #[test]
    fn parse_args_accepts_multiple_languages_before_video() {
        let config = parse_args([
            "--lang".into(),
            "en".into(),
            "es".into(),
            "https://youtu.be/dQw4w9WgXcQ".into(),
        ])
        .unwrap();
        assert_eq!(config.languages, ["en", "es"]);
    }

    #[test]
    fn parse_args_accepts_multiple_languages_after_video() {
        let config = parse_args([
            "https://youtu.be/dQw4w9WgXcQ".into(),
            "--lang".into(),
            "en".into(),
            "es".into(),
        ])
        .unwrap();
        assert_eq!(config.languages, ["en", "es"]);
    }

    #[test]
    fn parse_args_accepts_comma_separated_languages() {
        let config =
            parse_args(["--lang=en,es".into(), "https://youtu.be/dQw4w9WgXcQ".into()]).unwrap();
        assert_eq!(config.languages, ["en", "es"]);
    }

    #[test]
    fn format_timestamp_matches_python_version() {
        assert_eq!(format_timestamp(65.9), "1:05");
        assert_eq!(format_timestamp(3661.0), "1:01:01");
    }

    #[test]
    fn format_transcript_plain_text() {
        let snippets = vec![
            TranscriptItem {
                text: " hello ".to_string(),
                start: 0.0,
                duration: 1.0,
            },
            TranscriptItem {
                text: "".to_string(),
                start: 1.0,
                duration: 1.0,
            },
            TranscriptItem {
                text: "world".to_string(),
                start: 2.0,
                duration: 1.0,
            },
        ];

        assert_eq!(format_transcript(&snippets, false), "hello\nworld");
    }

    #[test]
    fn format_transcript_with_timestamps() {
        let snippets = vec![TranscriptItem {
            text: "hello".to_string(),
            start: 65.0,
            duration: 1.0,
        }];

        assert_eq!(format_transcript(&snippets, true), "[1:05] hello");
    }
}
