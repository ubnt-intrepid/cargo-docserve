#[macro_use]
extern crate askama;
extern crate cargo;
#[macro_use]
extern crate log;
extern crate failure;
extern crate http;
extern crate hyper;
extern crate mime_guess;
extern crate tokio;

mod doc;
mod serve;

// ====

use cargo::core::compiler::CompileMode;
use cargo::core::Workspace;
use cargo::ops::Packages;
use cargo::util::Config;

use failure::Fallible;

pub fn run(config: &Config, mode: CompileMode, spec: Packages) -> Fallible<()> {
    let root = cargo::util::important_paths::find_root_manifest_for_wd(config.cwd())?;
    let workspace = Workspace::new(&root, &config)?;
    doc::generate(&workspace, mode, spec)?;
    serve::serve(&workspace)?;
    Ok(())
}
