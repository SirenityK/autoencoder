mod complete;
mod encode;
mod ffmpeg;

use std::io::{self, Write};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::env::Shells;
use clap_complete::{CompleteEnv, Shell};

use crate::encode::{build_vf, encode_discord, encode_whatsapp, resolve_output};
use crate::ffmpeg::{preflight, probe_dims, PipeSafeWriter, Result};

#[derive(Parser)]
#[command(
    name = "autoencode",
    version,
    about = "Intelligent video encoder for WhatsApp and Discord",
    long_about = "A binary CLI tool to replace complex FFmpeg bash scripts.",
    disable_help_subcommand = true
)]
struct Cli {
    /// Encoder preset: 'whatsapp' or 'discord'
    #[arg(short, long, default_value = "whatsapp", value_parser = ["whatsapp", "discord"])]
    preset: String,

    /// FFmpeg tune option. Only applies to whatsapp preset.
    #[arg(short, long)]
    tune: Option<String>,

    /// Output filename (optional)
    #[arg(short, long, value_hint = clap::ValueHint::FilePath)]
    output: Option<String>,

    /// Target resolution of the shorter side (e.g., '720' or '720p')
    #[arg(short, long, default_value_t = 480, value_parser = parse_resolution)]
    resolution: u32,

    /// Frame rate cap, never raises the source rate (e.g., '60', 'ntsc', 'ntsc_film')
    #[arg(short, long, default_value = "ntsc")]
    fps: String,

    /// Pixel format. The discord preset only supports the yuv420p variants.
    #[arg(long, default_value = "yuv420p", value_parser = crate::complete::PIX_FMTS)]
    pix_fmt: String,

    /// Target video bitrate (e.g., '1000k')
    #[arg(short = 'b', long)]
    vbitrate: Option<String>,

    /// Target audio bitrate (e.g., '128k')
    #[arg(short = 'a', long)]
    abitrate: Option<String>,

    /// CRF value (default 28). Only applies to whatsapp preset.
    #[arg(short, long, default_value_t = 28, conflicts_with = "two_pass")]
    crf: u8,

    /// Enable two-pass encoding at --vbitrate (default 500k). Only applies to whatsapp preset.
    #[arg(long)]
    two_pass: bool,

    /// Encoder speed/quality preset (e.g., 'slow' for whatsapp, '5' for discord)
    #[arg(short = 'e', long)]
    enc_preset: Option<String>,

    /// Input files
    #[arg(value_name = "input_files", value_hint = clap::ValueHint::FilePath)]
    inputs: Vec<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate completion script
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn parse_resolution(value: &str) -> std::result::Result<u32, String> {
    value
        .strip_suffix('p')
        .unwrap_or(value)
        .parse()
        .map_err(|_| format!("expected a number like 720 or 720p, got '{}'", value))
}

fn main() {
    // Answers the shell's tab-completion requests, then exits; no-op otherwise.
    CompleteEnv::with_factory(complete::command_from_env).complete();

    let cli = Cli::parse();

    if let Some(Commands::Completion { shell }) = cli.command {
        let shells = Shells::builtins();
        let completer = shells
            .completer(&shell.to_string())
            .expect("clap shell is a builtin");
        let script = completer.write_registration(
            "COMPLETE",
            "autoencode",
            "autoencode",
            "autoencode",
            &mut PipeSafeWriter(io::stdout()),
        );
        // Installed under fpath the file is autoloaded on first tab: complete now, not on the second tab.
        let script = script.and_then(|()| match shell {
            Shell::Zsh => io::stdout().write_all(
                b"\nif [ \"$funcstack[1]\" = \"_autoencode\" ]; then\n    _clap_dynamic_completer_autoencode \"$@\"\nfi\n",
            ),
            _ => Ok(()),
        });
        if let Err(err) = script {
            eprintln!("Error writing completion script: {}", err);
            std::process::exit(1);
        }
        return;
    }

    if cli.inputs.is_empty() {
        clap::Error::raw(
            clap::error::ErrorKind::MissingRequiredArgument,
            "the following required arguments were not provided:\n  <input_files>...",
        )
        .exit();
    }

    if cli.preset == "discord" && !cli.pix_fmt.starts_with("yuv420p") {
        Cli::command()
            .error(
                clap::error::ErrorKind::ValueValidation,
                format!(
                    "--pix-fmt {} is not supported by the discord preset (SVT-AV1 is 4:2:0 only)",
                    cli.pix_fmt
                ),
            )
            .exit();
    }

    let has_fdk = match preflight(&cli.preset) {
        Ok(has_fdk) => has_fdk,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };
    let mut failed = false;
    for input in &cli.inputs {
        if let Err(err) = process_video(input, &cli, has_fdk) {
            eprintln!("Error processing {}: {}", input, err);
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn process_video(input: &str, cli: &Cli, has_fdk: bool) -> Result<()> {
    println!("Processing: {} using preset [{}]", input, cli.preset);

    let dims = probe_dims(input).map_err(|e| format!("failed to probe video: {}", e))?;
    let vf = build_vf(&dims, cli.resolution, &cli.fps);
    let output = resolve_output(input, cli.output.as_deref(), &cli.preset);

    match cli.preset.as_str() {
        "whatsapp" => encode_whatsapp(input, &output, &vf, cli, has_fdk),
        "discord" => encode_discord(input, &output, &vf, cli),
        _ => unreachable!("preset validated at parse time"),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_resolution;

    #[test]
    fn resolution_accepts_optional_p_suffix() {
        assert_eq!(parse_resolution("720"), Ok(720));
        assert_eq!(parse_resolution("1080p"), Ok(1080));
        assert!(parse_resolution("hd").is_err());
    }
}
