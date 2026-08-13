use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::ffmpeg::{run, run_pipe_pass, Dimensions, Result, TempCleanup};

/// Flags applied to every video encode: bt709 color and yuv420p pixel format.
pub(crate) fn std_flags() -> [&'static str; 10] {
    [
        "-pix_fmt",
        "yuv420p",
        "-colorspace",
        "bt709",
        "-color_trc",
        "bt709",
        "-color_primaries",
        "bt709",
        "-color_range",
        "tv",
    ]
}

/// Cap fps at 29.97 (ntsc) and scale the longer side to `resolution` (even dims).
pub fn build_vf(dims: &Dimensions, resolution: u32) -> String {
    let scale = if dims.height > dims.width {
        format!("scale={}:-2", resolution)
    } else {
        format!("scale=-2:{}", resolution)
    };
    format!("fps=min(ntsc\\,source_fps),{}", scale)
}

pub fn resolve_output(input: &str, output: Option<&str>, preset: &str) -> String {
    if let Some(output) = output {
        return output.to_string();
    }
    let stem = Path::new(input)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.to_string());
    format!("{}_{}.mp4", stem, preset)
}

fn base_name(input: &str) -> String {
    Path::new(input)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.to_string())
}

// --- Whatsapp preset (libx264, mp4) ---

pub fn encode_whatsapp(input: &str, output: &str, vf: &str, cli: &crate::Cli, has_fdk: bool) -> Result<()> {
    let base = whatsapp_base(input, vf);
    if cli.two_pass {
        whatsapp_two_pass(&base, input, output, cli, has_fdk)
    } else {
        whatsapp_single_pass(&base, output, cli, has_fdk)
    }
}

fn whatsapp_base(input: &str, vf: &str) -> Vec<String> {
    vec![
        "-i".into(),
        input.into(),
        "-vf".into(),
        vf.into(),
        "-sws_flags".into(),
        "lanczos".into(),
    ]
}

fn whatsapp_audio_args(has_fdk: bool, abitrate: Option<&str>) -> Vec<String> {
    let (codec, default_bitrate) = if has_fdk {
        ("libfdk_aac", "32k")
    } else {
        ("aac", "96k")
    };
    let mut args = vec![
        "-c:a".into(),
        codec.into(),
        "-b:a".into(),
        abitrate.unwrap_or(default_bitrate).into(),
        "-ac".into(),
        "2".into(),
    ];
    if has_fdk {
        args.extend(["-profile:a".into(), "aac_he_v2".into()]);
    }
    args
}

fn whatsapp_video_args(enc_preset: &str) -> Vec<String> {
    let mut args = vec![
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        enc_preset.into(),
        "-movflags".into(),
        "+faststart".into(),
    ];
    args.extend(std_flags().map(String::from));
    args
}

fn whatsapp_single_pass(base: &[String], output: &str, cli: &crate::Cli, has_fdk: bool) -> Result<()> {
    let enc_preset = cli.enc_preset.as_deref().unwrap_or("slow");
    let mut args = vec!["-y".to_string()];
    args.extend_from_slice(base);
    args.extend(whatsapp_audio_args(has_fdk, cli.abitrate.as_deref()));
    args.extend(whatsapp_video_args(enc_preset));

    if let Some(bitrate) = &cli.vbitrate {
        args.extend(["-b:v".into(), bitrate.clone()]);
    } else {
        args.extend(["-crf".into(), cli.crf.to_string()]);
    }
    if let Some(tune) = &cli.tune {
        args.extend(["-tune".into(), tune.clone()]);
    }
    args.push(output.to_string());

    println!("Running FFmpeg (Whatsapp Single Pass)...");
    run(Command::new("ffmpeg").args(&args))
}

fn whatsapp_two_pass(base: &[String], input: &str, output: &str, cli: &crate::Cli, has_fdk: bool) -> Result<()> {
    let enc_preset = cli.enc_preset.as_deref().unwrap_or("slow");
    let bitrate = cli.vbitrate.as_deref().unwrap_or("1000k");

    let pass_log = std::env::temp_dir().join(format!(
        "ffmpeg2pass-{}-{}",
        base_name(input),
        std::process::id()
    ));
    // ffmpeg appends -0.log / -0.log.mbtree to the -passlogfile value.
    let log0 = format!("{}-0.log", pass_log.display());
    let log0_mbtree = format!("{}-0.log.mbtree", pass_log.display());
    let _cleanup = TempCleanup(vec![PathBuf::from(&log0), PathBuf::from(&log0_mbtree)]);

    // CRF is ignored in two-pass mode; bitrate is always explicit (default 1000k).
    let mut video = vec![
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        enc_preset.into(),
        "-movflags".into(),
        "+faststart".into(),
        "-b:v".into(),
        bitrate.into(),
    ];
    video.extend(std_flags().map(String::from));
    if let Some(tune) = &cli.tune {
        video.extend(["-tune".into(), tune.clone()]);
    }

    // Pass 1: no audio, stats only, output discarded.
    let mut pass1 = vec!["-y".to_string()];
    pass1.extend_from_slice(base);
    pass1.push("-an".to_string());
    pass1.extend(video.iter().cloned());
    pass1.extend(["-pass".into(), "1".into(), "-passlogfile".into(), pass_log.to_string_lossy().into_owned()]);
    pass1.extend(["-f".into(), "null".into(), "-".into()]);

    println!("Running FFmpeg (Whatsapp 2-Pass): Pass 1...");
    run(Command::new("ffmpeg").args(&pass1).stdout(Stdio::null()))?;

    // Pass 2: full audio + video, writes the output.
    let mut pass2 = vec!["-y".to_string()];
    pass2.extend_from_slice(base);
    pass2.extend(whatsapp_audio_args(has_fdk, cli.abitrate.as_deref()));
    pass2.extend(video);
    pass2.extend(["-pass".into(), "2".into(), "-passlogfile".into(), pass_log.to_string_lossy().into_owned()]);
    pass2.push(output.to_string());

    println!("Running FFmpeg (Whatsapp 2-Pass): Pass 2...");
    run(Command::new("ffmpeg").args(&pass2))
}

// --- Discord preset (SVT-AV1 + opus, mp4) ---

pub fn encode_discord(input: &str, output: &str, vf: &str, cli: &crate::Cli) -> Result<()> {
    // Discord is always two-pass; --two-pass is ignored here.
    let enc_preset = cli.enc_preset.as_deref().unwrap_or("5");
    let vbitrate = cli.vbitrate.as_deref().unwrap_or("256k");
    let abitrate = cli.abitrate.as_deref().unwrap_or("32k");

    let pid = std::process::id();
    let stats = std::env::temp_dir().join(format!("svtav1_stats.{}.stat", pid));
    let ivf = std::env::temp_dir().join(format!("temp_video.{}.ivf", pid));
    let _cleanup = TempCleanup(vec![stats.clone(), ivf.clone()]);

    println!("Running SVT-AV1 Pass 1...");
    run_pipe_pass(input, vf, vbitrate, enc_preset, &stats, None)?;

    println!("Running SVT-AV1 Pass 2...");
    run_pipe_pass(input, vf, vbitrate, enc_preset, &stats, Some(&ivf))?;

    println!("Muxing Audio and Video...");
    run(Command::new("ffmpeg").args([
        "-i",
        ivf.to_str().unwrap_or_default(),
        "-i",
        input,
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-c:v",
        "copy",
        "-c:a",
        "libopus",
        "-b:a",
        abitrate,
        "-ac",
        "2",
        "-y",
        output,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn vf_keeps_fps_cap_with_escaped_comma() {
        let vf = build_vf(&Dimensions { width: 640, height: 360 }, 480);
        assert_eq!(vf, "fps=min(ntsc\\,source_fps),scale=-2:480");
        assert!(vf.contains("\\,"));
    }

    #[test]
    fn vf_portrait_scales_height() {
        let vf = build_vf(&Dimensions { width: 360, height: 640 }, 240);
        assert_eq!(vf, "fps=min(ntsc\\,source_fps),scale=240:-2");
    }

    #[test]
    fn vf_square_uses_landscape_scale() {
        let vf = build_vf(&Dimensions { width: 640, height: 640 }, 480);
        assert_eq!(vf, "fps=min(ntsc\\,source_fps),scale=-2:480");
    }

    #[test]
    fn output_name_defaults_to_stem_plus_preset() {
        assert_eq!(resolve_output("video.mp4", None, "whatsapp"), "video_whatsapp.mp4");
        assert_eq!(resolve_output("dir/a.mov", None, "discord"), "a_discord.mp4");
        assert_eq!(resolve_output("x.mp4", Some("out.mp4"), "whatsapp"), "out.mp4");
    }

    #[test]
    fn whatsapp_audio_defaults() {
        assert_eq!(
            whatsapp_audio_args(false, None),
            args(&["-c:a", "aac", "-b:a", "96k", "-ac", "2"])
        );
        assert_eq!(
            whatsapp_audio_args(true, None),
            args(&[
                "-c:a",
                "libfdk_aac",
                "-b:a",
                "32k",
                "-ac",
                "2",
                "-profile:a",
                "aac_he_v2"
            ])
        );
    }

    #[test]
    fn whatsapp_audio_bitrate_override() {
        assert_eq!(
            whatsapp_audio_args(false, Some("128k")),
            args(&["-c:a", "aac", "-b:a", "128k", "-ac", "2"])
        );
    }

    #[test]
    fn std_flags_are_bt709_yuv420p() {
        assert_eq!(
            std_flags(),
            [
                "-pix_fmt",
                "yuv420p",
                "-colorspace",
                "bt709",
                "-color_trc",
                "bt709",
                "-color_primaries",
                "bt709",
                "-color_range",
                "tv"
            ]
        );
    }
}
