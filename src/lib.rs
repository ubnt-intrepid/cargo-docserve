#[macro_use]
extern crate askama;
extern crate cargo;
#[macro_use]
extern crate log;
extern crate failure;
extern crate futures;
extern crate http;
extern crate hyper;
extern crate mime_guess;
extern crate notify;
extern crate tokio;

pub mod doc;
pub mod server;

// ====

use cargo::core::compiler::CompileMode;
use cargo::core::Workspace;
use cargo::ops::Packages;
use cargo::util::important_paths::find_root_manifest_for_wd;
use cargo::util::{Config, Filesystem};

use std::net::SocketAddr;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use failure::Fallible;
use futures::sync::oneshot;
use futures::{future, Future};
use notify::{watcher, RecursiveMode, Watcher};

use self::server::ServerConfig;

#[derive(Debug)]
pub struct DocserveOptions {
    pub config: Config,
    pub mode: CompileMode,
    pub spec: Packages,
    pub watch: bool,
    pub addr: SocketAddr,
}

pub fn run(opts: DocserveOptions) -> Fallible<()> {
    let config = &opts.config;
    let root = find_root_manifest_for_wd(config.cwd())?;
    let workspace = Workspace::new(&root, &opts.config)?;

    let target_dir = config
        .target_dir()?
        .map_or("./target".into(), Filesystem::into_path_unlocked);
    let doc_dir = target_dir.join("doc");

    let targets = workspace
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
    let server_config = Arc::new(ServerConfig { doc_dir, targets });

    trace!("starting the filesystem notifier");
    let (tx_notify, rx_notify) = mpsc::channel();
    let mut watcher = watcher(tx_notify, Duration::from_millis(500))?;
    watcher.watch(workspace.root().join("src"), RecursiveMode::Recursive)?;

    config
        .shell()
        .status("Docserve", "Generating the documentation")?;
    doc::generate(&workspace, opts.mode, opts.spec.clone())?;

    loop {
        config.shell().status(
            "Docserve",
            format!("Starting HTTP server listening on http://{}", opts.addr),
        )?;

        if !opts.watch {
            trace!("--> entered in standard mode");
            server::start(&opts.addr, server_config.clone(), future::empty::<(), ()>())?;
            break Ok(());
        }

        trace!("--> entered in watch mode");
        let (tx_shutdown, rx_shutdown) = oneshot::channel();
        let (tx_done, rx_done) = oneshot::channel();
        std::thread::spawn({
            let addr = opts.addr;
            let server_config = server_config.clone();
            move || {
                let _ = server::start(&addr, server_config, rx_shutdown);
                tx_done.send(()).unwrap();
            }
        });

        match rx_notify.recv() {
            Ok(ev) => {
                trace!("Received event: {:?}", ev);

                // send shutdown signal and wait for it.
                config
                    .shell()
                    .status("Docserve", "Shutdown the HTTP server")?;
                tx_shutdown.send(()).unwrap();
                rx_done.wait().unwrap();

                config
                    .shell()
                    .status("Docserve", "Regenerating the documentation")?;
                doc::generate(&workspace, opts.mode, opts.spec.clone())?;

                continue;
            }
            Err(err) => {
                error!("watch error: {}", err);
                std::process::exit(1);
            }
        }
    }
}
