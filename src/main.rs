mod encode;
mod ffmpeg;

use std::io;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use crate::encode::{build_vf, encode_discord, encode_whatsapp, resolve_output};
use crate::ffmpeg::{has_lib_fdk, probe_dims, PipeSafeWriter, Result};

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
    #[arg(short, long)]
    output: Option<String>,

    /// Target resolution
    #[arg(short, long, default_value_t = 480)]
    resolution: u32,

    /// Target video bitrate (e.g., '1000k')
    #[arg(short = 'b', long)]
    vbitrate: Option<String>,

    /// Target audio bitrate (e.g., '128k')
    #[arg(short = 'a', long)]
    abitrate: Option<String>,

    /// CRF value (default 28). Only applies to whatsapp preset.
    #[arg(short, long, default_value_t = 28)]
    crf: u8,

    /// Enable two-pass encoding
    #[arg(long)]
    two_pass: bool,

    /// Encoder speed/quality preset (e.g., 'slow' for whatsapp, '5' for discord)
    #[arg(short = 'e', long)]
    enc_preset: Option<String>,

    /// Input files
    #[arg(value_name = "input_files")]
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

fn main() {
    let cli = Cli::parse();

    if let Some(Commands::Completion { shell }) = cli.command {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "autoencode", &mut PipeSafeWriter(io::stdout()));
        return;
    }

    if cli.inputs.is_empty() {
        clap::Error::raw(
            clap::error::ErrorKind::MissingRequiredArgument,
            "the following required arguments were not provided:\n  <input_files>...",
        )
        .exit();
    }

    let has_fdk = has_lib_fdk();
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
    let vf = build_vf(&dims, cli.resolution);
    let output = resolve_output(input, cli.output.as_deref(), &cli.preset);

    match cli.preset.as_str() {
        "whatsapp" => encode_whatsapp(input, &output, &vf, cli, has_fdk),
        "discord" => encode_discord(input, &output, &vf, cli),
        _ => unreachable!("preset validated at parse time"),
    }
}
