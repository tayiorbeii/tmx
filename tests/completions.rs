use std::process::{Command, Output};

fn run_tmx(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tmx"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn completion_command_writes_each_supported_shell_to_stdout() {
    for shell in ["bash", "zsh", "fish"] {
        let output = run_tmx(&["completions", shell]);
        assert!(output.status.success(), "generation failed for {shell}");
        assert!(output.stderr.is_empty(), "unexpected stderr for {shell}");
        assert!(!output.stdout.is_empty(), "empty stdout for {shell}");

        let script = String::from_utf8(output.stdout).unwrap();
        for expected in ["palette", "completions", "session"] {
            assert!(
                script.contains(expected),
                "{shell} completions are missing {expected:?}"
            );
        }
        let option_spellings = if shell == "fish" {
            ["-l desktop", "-l set"]
        } else {
            ["--desktop", "--set"]
        };
        for expected in option_spellings {
            assert!(
                script.contains(expected),
                "{shell} completions are missing option {expected:?}"
            );
        }
    }
}

#[test]
fn invalid_completion_shell_reports_an_error_on_stderr() {
    let output = run_tmx(&["completions", "powershell"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid value 'powershell'"));
    assert!(stderr.contains("bash"));
    assert!(stderr.contains("zsh"));
    assert!(stderr.contains("fish"));
}
