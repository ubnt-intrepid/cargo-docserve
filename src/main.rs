#![warn(unused)]

extern crate cargo;
extern crate cargo_docserve;

extern crate failure;
#[macro_use]
extern crate log;
extern crate pretty_env_logger;
#[macro_use]
extern crate structopt;

use cargo::core::compiler::CompileMode;
use cargo::ops::Packages;
use cargo::util::Config;

use failure::Fallible;
use std::path::PathBuf;
use structopt::clap::AppSettings;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(bin_name = "cargo")]
enum Opts {
    #[structopt(
        name = "docserve",
        raw(
            setting = "AppSettings::UnifiedHelpMessage",
            setting = "AppSettings::DeriveDisplayOrder",
            setting = "AppSettings::DontCollapseArgsInUsage"
        )
    )]
    /// serve API docs
    Docserve(CliOptions),
}

#[derive(Debug, StructOpt)]
#[structopt(name = "cargo-docserve")]
struct CliOptions {
    #[structopt(long = "all")]
    all: bool,

    #[structopt(long = "exclude", value_name = "SPEC")]
    exclude: Vec<String>,

    #[structopt(short = "p", long = "package", value_name = "SPEC")]
    package: Vec<String>,

    #[structopt(long = "no-deps")]
    no_deps: bool,

    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: u32,

    #[structopt(short = "q", long = "quiet")]
    quiet: Option<bool>,

    #[structopt(long = "color", value_name = "WHEN")]
    color: Option<String>,

    #[structopt(long = "frozen")]
    frozen: bool,

    #[structopt(long = "locked")]
    locked: bool,

    #[structopt(long = "target-dir", value_name = "DIRECTORY", parse(from_os_str))]
    target_dir: Option<PathBuf>,

    #[structopt(short = "Z", value_name = "FLAGS")]
    unstable_flags: Vec<String>,

    #[structopt(short = "w", long = "watch")]
    watch: bool,
}

fn main() -> Fallible<()> {
    pretty_env_logger::try_init()?;

    let opts = match Opts::from_args() {
        Opts::Docserve(opts) => opts,
    };
    debug!("cli options = {:?}", opts);

    let mode = CompileMode::Doc {
        deps: !opts.no_deps,
    };

    let spec = Packages::from_flags(opts.all, opts.exclude.clone(), opts.package.clone())?;

    let mut config = Config::default()?;
    config.configure(
        opts.verbose,
        opts.quiet,
        &opts.color,
        opts.frozen,
        opts.locked,
        &opts.target_dir,
        &opts.unstable_flags[..],
    )?;

    cargo_docserve::run(&config, mode, spec, opts.watch)?;

    Ok(())
}
