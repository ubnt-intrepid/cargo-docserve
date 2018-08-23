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
use cargo::util::{Config, Filesystem};

use std::sync::mpsc;

use failure::Fallible;
use futures::sync::oneshot;
use futures::Future;

pub fn run(config: &Config, mode: CompileMode, spec: Packages) -> Fallible<()> {
    let root = cargo::util::important_paths::find_root_manifest_for_wd(config.cwd())?;
    let workspace = Workspace::new(&root, &config)?;

    let addr = ([127, 0, 0, 1], 8000).into();

    // start filesystem notifier.
    use notify::Watcher;
    let (tx_notify, rx_notify) = mpsc::channel();
    let mut watcher = notify::watcher(tx_notify, std::time::Duration::from_millis(500))?;
    watcher.watch(
        workspace.root().join("src"),
        notify::RecursiveMode::Recursive,
    )?;

    config
        .shell()
        .status("Docserve", "Generating the documentation")?;
    doc::generate(&workspace, mode, spec.clone())?;

    loop {
        config.shell().status(
            "Docserve",
            format!("Starting HTTP server listening on http://{}", addr),
        )?;
        let target_dir = workspace
            .config()
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
        let server_config = server::ServerConfig {
            doc_dir,
            targets,
            addr,
        };
        let (tx_shutdown, rx_shutdown) = oneshot::channel();
        let (tx_done, rx_done) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = server::start(server_config, rx_shutdown.map_err(|_| ()));
            tx_done.send(()).unwrap();
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
                doc::generate(&workspace, mode, spec.clone())?;

                continue;
            }
            Err(err) => {
                error!("watch error: {}", err);
                std::process::exit(1);
            }
        }
    }
}
