use clap::CommandFactory;
use clap_complete::Shell;

use crate::Cli;

pub fn generate(shell: &str) {
    let shell = match shell {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "elvish" => Shell::Elvish,
        "powershell" => Shell::PowerShell,
        other => {
            eprintln!(
                "Unsupported shell: {}. Supported: bash, zsh, fish, elvish, powershell",
                other
            );
            std::process::exit(1);
        }
    };

    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, &name, &mut std::io::stdout());
}
