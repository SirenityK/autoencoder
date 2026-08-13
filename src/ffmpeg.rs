use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::encode::std_flags;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Copy)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

/// Removes temp files on drop, even when the encode fails midway.
pub struct TempCleanup(pub Vec<PathBuf>);

impl Drop for TempCleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

/// clap_complete panics on write errors; treat a closed pipe (e.g. `| head`) as success.
pub struct PipeSafeWriter<W>(pub W);

impl<W: io::Write> io::Write for PipeSafeWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0.write(buf) {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(buf.len()),
            r => r,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0.flush() {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            r => r,
        }
    }
}

pub fn probe_dims(input: &str) -> Result<Dimensions> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            input,
        ])
        .output()?;
    if !out.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let (w, h) = match raw.trim().split_once('x') {
        Some(parts) => parts,
        None => return Err(format!("unexpected ffprobe output: {}", raw.trim()).into()),
    };
    Ok(Dimensions {
        width: w.trim().parse()?,
        height: h.trim().parse()?,
    })
}

pub fn has_lib_fdk() -> bool {
    Command::new("ffmpeg")
        .arg("-encoders")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("libfdk_aac"))
        .unwrap_or(false)
}

pub fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} exited with {}",
            cmd.get_program().to_string_lossy(),
            status
        )
        .into())
    }
}

pub fn run_pipe_pass(
    input: &str,
    vf: &str,
    vbitrate: &str,
    enc_preset: &str,
    stats: &Path,
    output: Option<&Path>,
) -> Result<()> {
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg
        .args(["-i", input, "-map", "0:v", "-vf", vf])
        .args(std_flags())
        .args(["-f", "yuv4mpegpipe", "-"])
        .stdout(Stdio::piped());
    let mut ffmpeg = ffmpeg
        .spawn()
        .map_err(|e| format!("failed to start ffmpeg: {}", e))?;
    let pipe_in = ffmpeg.stdout.take().expect("piped stdout");

    let pass = if output.is_some() { "2" } else { "1" };
    let mut svt = Command::new("SvtAv1EncApp");
    svt.args([
        "-i",
        "-",
        "--preset",
        enc_preset,
        "--rc",
        "1",
        "--tbr",
        vbitrate,
        "--stats",
        stats.to_str().unwrap_or_default(),
        "--pass",
        pass,
    ]);
    if let Some(out) = output {
        svt.args(["-b", out.to_str().unwrap_or_default()]);
    }

    let svt_status = match svt.stdin(Stdio::from(pipe_in)).status() {
        Ok(status) => status,
        Err(err) => {
            let _ = ffmpeg.kill();
            let _ = ffmpeg.wait();
            return Err(format!("failed to run SvtAv1EncApp: {}", err).into());
        }
    };
    let ffmpeg_status = ffmpeg.wait()?;
    if !svt_status.success() {
        return Err(format!("SvtAv1EncApp exited with {}", svt_status).into());
    }
    if !ffmpeg_status.success() {
        return Err(format!("ffmpeg exited with {}", ffmpeg_status).into());
    }
    Ok(())
}
