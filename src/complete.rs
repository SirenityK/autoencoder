use std::ffi::OsString;

use clap::parser::ValueSource;
use clap::{Command, CommandFactory};
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};

use crate::Cli;

pub const PIX_FMTS: [&str; 6] = [
    "yuv420p",
    "yuv420p10le",
    "yuv422p",
    "yuv422p10le",
    "yuv444p",
    "yuv444p10le",
];

type Choices = &'static [(&'static str, &'static str)];

const RESOLUTIONS: Choices = &[
    ("2160p", "4K"),
    ("1440p", "QHD"),
    ("1080p", "Full HD"),
    ("720p", "HD"),
    ("480p", "SD (default)"),
    ("360p", ""),
    ("240p", ""),
    ("144p", ""),
];
const FPS: Choices = &[
    ("ntsc_film", "23.976"),
    ("ntsc", "29.97 (default)"),
    ("60", ""),
];
const X264_PRESETS: Choices = &[
    ("ultrafast", ""),
    ("superfast", ""),
    ("veryfast", ""),
    ("faster", ""),
    ("fast", ""),
    ("medium", ""),
    ("slow", "default"),
    ("slower", ""),
    ("veryslow", ""),
    ("placebo", ""),
];
const SVT_PRESETS: Choices = &[
    ("0", "slowest"),
    ("1", ""),
    ("2", ""),
    ("3", ""),
    ("4", ""),
    ("5", "default"),
    ("6", ""),
    ("7", ""),
    ("8", ""),
    ("9", ""),
    ("10", ""),
    ("11", ""),
    ("12", ""),
    ("13", "fastest"),
];
const PIX_FMTS_420: Choices = &[("yuv420p", "8-bit (default)"), ("yuv420p10le", "10-bit")];
const PIX_FMTS_ALL: Choices = &[
    ("yuv420p", "8-bit 4:2:0 (default)"),
    ("yuv420p10le", "10-bit 4:2:0"),
    ("yuv422p", "8-bit 4:2:2"),
    ("yuv422p10le", "10-bit 4:2:2"),
    ("yuv444p", "8-bit 4:4:4"),
    ("yuv444p10le", "10-bit 4:4:4"),
];

fn candidates(choices: Choices) -> ArgValueCandidates {
    ArgValueCandidates::new(move || {
        choices
            .iter()
            .map(|(value, help)| {
                let help = (!help.is_empty()).then(|| (*help).into());
                CompletionCandidate::new(*value).help(help)
            })
            .collect()
    })
}

/// The CLI as seen by tab completion: `words` is the command line being completed,
/// starting at the binary name and ending at the word under the cursor.
fn command_for(words: &[OsString]) -> Command {
    // Reuse clap's parser so -p x, -px, --preset x and --preset=x all count.
    let matches = Cli::command()
        .ignore_errors(true)
        .try_get_matches_from(words)
        .ok();
    let discord = matches
        .as_ref()
        .and_then(|m| m.get_one::<String>("preset"))
        .is_some_and(|p| p == "discord");
    // Not get_flag: it panics when an unfinished word (`-r ""`) stops parsing before defaults apply.
    let typed = |id| {
        matches
            .as_ref()
            .is_some_and(|m| m.value_source(id) == Some(ValueSource::CommandLine))
    };
    let two_pass = typed("two_pass");
    let crf_given = typed("crf");

    let mut cmd = Cli::command()
        .mut_arg("resolution", |a| a.add(candidates(RESOLUTIONS)))
        .mut_arg("fps", |a| a.add(candidates(FPS)))
        .mut_arg("enc_preset", |a| {
            a.add(candidates(if discord { SVT_PRESETS } else { X264_PRESETS }))
        })
        .mut_arg("pix_fmt", |a| {
            a.add(candidates(if discord {
                PIX_FMTS_420
            } else {
                PIX_FMTS_ALL
            }))
        });
    if discord {
        for arg in ["tune", "crf", "two_pass"] {
            cmd = cmd.mut_arg(arg, |a| a.hide(true));
        }
    }
    // --crf and --two-pass conflict, so don't offer one once the other is typed.
    if two_pass {
        cmd = cmd.mut_arg("crf", |a| a.hide(true));
    }
    if crf_given {
        cmd = cmd.mut_arg("two_pass", |a| a.hide(true));
    }
    // `completion` only makes sense as the very first word.
    if words.len() > 2 {
        cmd = cmd.mut_subcommand("completion", |c| c.hide(true));
    }
    cmd
}

/// Factory for `CompleteEnv`, which calls us as `autoencode -- <words...>`.
pub fn command_from_env() -> Command {
    let words: Vec<OsString> = std::env::args_os()
        .skip_while(|a| a != "--")
        .skip(1)
        .collect();
    command_for(&words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(line: &[&str]) -> Vec<String> {
        let words: Vec<OsString> = line.iter().map(OsString::from).collect();
        let mut cmd = command_for(&words);
        clap_complete::engine::complete(&mut cmd, words.clone(), words.len() - 1, None)
            .unwrap()
            .iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect()
    }

    fn has(values: &[String], value: &str) -> bool {
        values.iter().any(|v| v == value)
    }

    #[test]
    fn discord_hides_whatsapp_only_flags() {
        for line in [
            &["autoencode", "-p", "discord", "--"][..],
            &["autoencode", "--preset=discord", "--"][..],
            &["autoencode", "-pdiscord", "--"][..],
        ] {
            let flags = complete(line);
            assert!(has(&flags, "--vbitrate"), "{:?}", flags);
            for hidden in ["--crf", "--tune", "--two-pass"] {
                assert!(!has(&flags, hidden), "{:?}", flags);
            }
        }
        assert!(has(&complete(&["autoencode", "--"]), "--crf"));
    }

    #[test]
    fn enc_preset_follows_preset_and_default() {
        let default = complete(&["autoencode", "-e", ""]);
        assert!(has(&default, "slow") && !has(&default, "5"));
        let discord = complete(&["autoencode", "-p", "discord", "-e", ""]);
        assert!(has(&discord, "5") && !has(&discord, "slow"));
    }

    #[test]
    fn two_pass_and_crf_hide_each_other() {
        let two_pass = complete(&["autoencode", "--two-pass", "--"]);
        assert!(
            !has(&two_pass, "--crf") && has(&two_pass, "--vbitrate"),
            "{:?}",
            two_pass
        );
        let crf = complete(&["autoencode", "--crf", "20", "--"]);
        assert!(!has(&crf, "--two-pass"), "{:?}", crf);
    }

    #[test]
    fn pix_fmt_follows_preset() {
        assert!(has(&complete(&["autoencode", "--pix-fmt", ""]), "yuv444p"));
        assert!(!has(
            &complete(&["autoencode", "-p", "discord", "--pix-fmt", ""]),
            "yuv444p"
        ));
    }

    #[test]
    fn value_lists() {
        assert!(has(&complete(&["autoencode", "-r", ""]), "1080p"));
        let fps = complete(&["autoencode", "-f", ""]);
        for value in ["ntsc_film", "ntsc", "60"] {
            assert!(has(&fps, value), "{:?}", fps);
        }
    }

    #[test]
    fn after_a_file_offers_flags_not_the_subcommand() {
        let first = complete(&["autoencode", ""]);
        assert!(has(&first, "completion"));
        for line in [
            &["autoencode", "a.mp4", ""][..],
            &["autoencode", "a.mp4", "-e", "slow", ""][..],
        ] {
            let next = complete(line);
            assert!(has(&next, "--preset"), "{:?}", next);
            assert!(!has(&next, "completion"), "{:?}", next);
        }
    }

    #[test]
    fn pix_fmt_lists_agree() {
        let listed: Vec<&str> = PIX_FMTS_ALL.iter().map(|c| c.0).collect();
        assert_eq!(listed, PIX_FMTS);
    }
}
