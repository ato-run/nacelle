mod commands;
mod error;
mod manifest;
mod observability;
mod package;
mod runtime;
mod signing;

use anyhow::Result;
use clap::Parser;

fn main() {
    if let Err(err) = Cli::parse().run() {
        eprintln!("Error: {}", err);
        let mut source = err.source();
        while let Some(cause) = source {
            eprintln!("  caused by: {}", cause);
            source = cause.source();
        }
        std::process::exit(1);
    }
}

#[derive(Parser)]
#[command(
    name = "adep",
    version,
    about = "ADEP package tooling",
    author = "ADEP Project"
)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

impl Cli {
    fn run(self) -> Result<()> {
        match self.command {
            commands::Command::Init(args) => commands::init::run(&args),
            commands::Command::Keygen(args) => commands::keygen::run(&args),
            commands::Command::Build(args) => commands::build::run(&args),
            commands::Command::Sign(args) => commands::sign::run(&args),
            commands::Command::Verify(args) => commands::verify::run(&args),
            commands::Command::Pack(args) => commands::pack::run(&args),
            commands::Command::Run(args) => commands::run::run(&args),
            commands::Command::Doctor(args) => commands::doctor::run(&args),
            commands::Command::Compose(args) => commands::compose::compose_command(args),
            commands::Command::DevPin(args) => commands::dev_pin::run(args),
            commands::Command::Manifest(args) => commands::manifest::run(args),
            commands::Command::Capsule(cmd) => commands::capsule::run(cmd),
            commands::Command::Deps(args) => commands::deps::run(args),
        }
    }
}
