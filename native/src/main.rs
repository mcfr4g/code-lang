use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "Usage: code-lang [--help] [--version] <file.code>";

fn main() -> ExitCode {
    let mut file = None;
    let mut positional = false;
    for argument in env::args_os().skip(1) {
        if !positional {
            match argument.to_str() {
                Some("-h" | "--help") => {
                    println!("Run a Code script with the native interpreter.\n\n{USAGE}");
                    return ExitCode::SUCCESS;
                }
                Some("--version") => {
                    println!("Code {}", env!("CARGO_PKG_VERSION"));
                    return ExitCode::SUCCESS;
                }
                Some("--") => {
                    positional = true;
                    continue;
                }
                Some(option) if option.starts_with('-') => {
                    eprintln!("{USAGE}\nUnrecognised option {option}");
                    return ExitCode::from(2);
                }
                _ => {}
            }
        }
        if file.is_some() {
            eprintln!("{USAGE}\nExpected a single script file");
            return ExitCode::from(2);
        }
        file = Some(PathBuf::from(argument));
    }

    let Some(file) = file else {
        eprintln!("{USAGE}\nExpected a script file");
        return ExitCode::from(2);
    };
    let source = match fs::read_to_string(&file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", file.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) =
        code_lang::run_with_input(&source, &mut io::stdin().lock(), &mut io::stdout().lock())
    {
        eprintln!("{}: {error}", file.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
