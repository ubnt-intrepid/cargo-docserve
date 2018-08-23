#[macro_use]
extern crate askama;
extern crate cargo;
extern crate clap_verbosity_flag;
extern crate failure;
extern crate futures;
extern crate http;
extern crate hyper;
#[macro_use]
extern crate log;
extern crate notify;
extern crate pretty_env_logger;
#[macro_use]
extern crate structopt;
extern crate mime_guess;
extern crate tokio;

use askama::Template;
use cargo::core::compiler::CompileMode;
use cargo::core::{Target, Workspace};
use cargo::ops::{CompileOptions, DocOptions};
use cargo::util::{Config, Filesystem};
use clap_verbosity_flag::Verbosity;
use failure::Fallible;
use futures::{future, Future};
use http::{Method, Response, StatusCode};
use hyper::body::Body;
use log::Level;
use mime_guess::guess_mime_type;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::read_to_end;

#[derive(Debug, StructOpt)]
#[structopt(name = "cargo-docserve")]
struct CliOptions {
    #[structopt(long = "no-deps")]
    no_deps: bool,

    #[structopt(flatten)]
    verbose: Verbosity,

    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    #[structopt(long = "color")]
    color: Option<String>,

    #[structopt(long = "frozen")]
    frozen: bool,

    #[structopt(long = "locked")]
    locked: bool,

    #[structopt(long = "target-dir", parse(from_os_str))]
    target_dir: Option<PathBuf>,

    #[structopt(short = "Z")]
    unstable_flags: Vec<String>,
}

fn main() -> Fallible<()> {
    pretty_env_logger::try_init()?;

    let opts = CliOptions::from_args();
    debug!("cli options = {:?}", opts);
    let verbosity = match opts.verbose.log_level() {
        Level::Info => 1,
        Level::Debug => 2,
        Level::Trace => 3,
        _ => 0,
    };

    let mut config = Config::default()?;
    config.configure(
        verbosity,
        Some(opts.quiet),
        &opts.color,
        opts.frozen,
        opts.locked,
        &opts.target_dir,
        &opts.unstable_flags[..],
    )?;
    let mode = CompileMode::Doc {
        deps: !opts.no_deps,
    };
    let compile_opts = CompileOptions::new(&config, mode)?;

    let root = cargo::util::important_paths::find_root_manifest_for_wd(config.cwd())?;
    let workspace = Workspace::new(&root, &config)?;

    let documented_targets: Vec<_> = workspace
        .members()
        .flat_map(|pkg| {
            pkg.manifest().targets().iter().filter_map(|t| {
                if t.documented() && t.is_lib() {
                    Some(t.clone())
                } else {
                    None
                }
            })
        })
        .collect();
    let documented_targets = Arc::new(documented_targets);

    let doc_opts = DocOptions {
        open_result: false,
        compile_opts,
    };
    cargo::ops::doc(&workspace, &doc_opts)?;

    let target_dir = config
        .target_dir()?
        .map_or("./target".into(), Filesystem::into_path_unlocked);
    let doc_dir = target_dir.join("doc");
    let doc_dir = Arc::new(doc_dir);

    let new_service = move || {
        let doc_dir = doc_dir.clone();
        let targets = documented_targets.clone();
        hyper::service::service_fn(
            move |req| -> Box<Future<Item = _, Error = io::Error> + Send> {
                if req.method() != Method::GET {
                    return Box::new(future::ok(
                        Response::builder()
                            .status(StatusCode::METHOD_NOT_ALLOWED)
                            .body(Body::default())
                            .unwrap(),
                    ));
                }

                if req.uri().path() == "/" || req.uri().path() == "/index.html" {
                    trace!("listed targets = {:?}", targets);
                    let rendered = render(&targets);
                    return Box::new(future::ok(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Body::from(rendered))
                            .unwrap(),
                    ));
                }

                let mut path = doc_dir.join(req.uri().path().trim_left_matches('/'));
                if path.is_dir() {
                    path.push("index.html");
                }
                trace!("path = {}", path.display());

                let content_type = guess_mime_type(&path);
                trace!("guessed content type: {}", content_type);

                Box::new(
                    File::open(path)
                        .and_then(|file| read_to_end(file, Vec::new()))
                        .then(move |result| match result {
                            Ok((_file, content)) => Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", content_type.as_ref())
                                .body(Body::from(content))
                                .unwrap()),
                            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                                Ok(Response::builder()
                                    .status(StatusCode::NOT_FOUND)
                                    .header("cache-control", "no-cache")
                                    .header("connection", "close")
                                    .body(Body::from(format!("{}", e)))
                                    .unwrap())
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::PermissionDenied => {
                                Ok(Response::builder()
                                    .status(StatusCode::FORBIDDEN)
                                    .header("cache-control", "no-cache")
                                    .header("connection", "close")
                                    .body(Body::from(format!("{}", e)))
                                    .unwrap())
                            }
                            Err(e) => Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .header("cache-control", "no-cache")
                                .header("connection", "close")
                                .body(Body::from(format!("I/O error: {}", e)))
                                .unwrap()),
                        }),
                )
            },
        )
    };

    let addr = ([127, 0, 0, 1], 8000).into();
    let server = hyper::server::Server::bind(&addr)
        .serve(new_service)
        .map_err(|e| error!("server error: {}", e));

    config
        .shell()
        .status("Docserve", format!("Listening on http://{}", addr))?;
    hyper::rt::run(server);

    Ok(())
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Targets<'a> {
    targets: &'a [Target],
}

fn render(targets: &[Target]) -> String {
    Targets { targets }.render().unwrap()
}
