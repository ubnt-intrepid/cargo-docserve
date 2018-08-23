use cargo::core::compiler::CompileMode;
use cargo::core::Workspace;
use cargo::ops;
use cargo::ops::{CompileOptions, DocOptions, Packages};

use failure::Fallible;

pub fn generate(ws: &Workspace, mode: CompileMode, spec: Packages) -> Fallible<()> {
    let mut compile_opts = CompileOptions::new(ws.config(), mode)?;
    compile_opts.spec = spec;

    let doc_opts = DocOptions {
        open_result: false,
        compile_opts,
    };

    ops::doc(&ws, &doc_opts)?;

    Ok(())
}
