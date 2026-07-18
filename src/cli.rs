use std::io::Write;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};

#[derive(Debug, Parser)]
#[command(
    name = "tmx",
    version,
    about = "A lightweight tmux-native workflow layer"
)]
pub struct Cli {
    /// Force mobile-friendly UI profile.
    #[arg(long, global = true, conflicts_with = "desktop")]
    pub mobile: bool,

    /// Force desktop UI profile.
    #[arg(long, global = true, conflicts_with = "mobile")]
    pub desktop: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the tmux palette.
    Palette(PaletteArgs),
    /// List live tmux sessions, windows, and panes.
    Ls,
    /// Switch/attach to a target.
    View(ViewArgs),
    /// Create-or-attach a session in the origin/current directory.
    New(NewArgs),
    /// Jump to the previous exact live target from tmx MRU.
    Last,
    /// Choose from recent tmx targets.
    Recent(PaletteArgs),
    /// Read or set a scoped note.
    Note(NoteArgs),
    /// Rename current scoped target.
    Rename(RenameArgs),
    /// Check dependencies, paths, and tmux capabilities.
    Doctor,
    /// Generate a shell completion script on stdout.
    Completions(CompletionsArgs),
}

#[derive(Debug, Args, Clone)]
pub struct CompletionsArgs {
    /// Shell for which to generate completions.
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl From<CompletionShell> for Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Zsh => Shell::Zsh,
            CompletionShell::Fish => Shell::Fish,
        }
    }
}

pub fn generate_completions(shell: CompletionShell, buffer: &mut dyn Write) {
    let mut command = Cli::command();
    generate(Shell::from(shell), &mut command, "tmx", buffer);
}

#[derive(Debug, Args, Default, Clone)]
pub struct PaletteArgs {
    /// Force desktop UI profile.
    #[arg(long, conflicts_with = "mobile")]
    pub desktop: bool,

    /// Force mobile-friendly UI profile.
    #[arg(long, conflicts_with = "desktop")]
    pub mobile: bool,
}

#[derive(Debug, Args)]
pub struct ViewArgs {
    /// tmux ID ($session, @window, %pane) or session name. If omitted, opens palette.
    pub target: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct NewArgs {
    /// Explicit session name.
    #[arg(long)]
    pub name: Option<String>,

    /// Label for a duplicate/labeled session of this directory.
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct NoteArgs {
    /// Note scope.
    #[arg(value_enum, default_value_t = ScopeArg::Session)]
    pub scope: ScopeArg,

    /// Set note text. If omitted, prints the current note.
    #[arg(long)]
    pub set: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct RenameArgs {
    /// Rename scope.
    #[arg(value_enum, default_value_t = ScopeArg::Session)]
    pub scope: ScopeArg,

    /// New name/title. If omitted, prompts on stdin.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    Session,
    Window,
    Pane,
}

impl ScopeArg {
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeArg::Session => "session",
            ScopeArg::Window => "window",
            ScopeArg::Pane => "pane",
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    #[test]
    fn parses_completions_command() {
        let cli = Cli::try_parse_from(["tmx", "completions", "bash"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Completions(CompletionsArgs {
                shell: CompletionShell::Bash
            }))
        ));
    }

    #[test]
    fn rejects_unsupported_completion_shell() {
        let error = Cli::try_parse_from(["tmx", "completions", "powershell"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn generates_nonempty_completions_from_cli_definition() {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
        ] {
            let mut output = Vec::new();
            generate_completions(shell, &mut output);
            let script = String::from_utf8(output).unwrap();

            assert!(!script.is_empty(), "empty completion output for {shell:?}");
            for expected in ["palette", "completions", "session"] {
                assert!(
                    script.contains(expected),
                    "{shell:?} completions are missing {expected:?}"
                );
            }
            let option_spellings = match shell {
                CompletionShell::Fish => ["-l desktop", "-l set"],
                CompletionShell::Bash | CompletionShell::Zsh => ["--desktop", "--set"],
            };
            for expected in option_spellings {
                assert!(
                    script.contains(expected),
                    "{shell:?} completions are missing option {expected:?}"
                );
            }
        }
    }
}
