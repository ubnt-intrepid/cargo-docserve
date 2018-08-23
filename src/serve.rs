use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use cargo::core::{Target, Workspace};
use cargo::util::Filesystem;

use askama::Template;
use failure::Fallible;
use http::{Method, Request, Response, StatusCode};
use hyper;
use hyper::service::Service;
use hyper::Body;
use mime_guess::guess_mime_type;
use tokio::fs::File;
use tokio::io::read_to_end;
use tokio::prelude::{future, Future};

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct TargetsTemplate<'a> {
    targets: &'a [Target],
}

#[derive(Debug)]
struct ServiceContext {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    doc_dir: PathBuf,
    targets: Vec<Target>,
}

impl ServiceContext {
    fn render_index(&self) -> Response<Body> {
        let t = TargetsTemplate {
            targets: &self.inner.targets,
        };
        let rendered = t.render().unwrap();
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(rendered))
            .unwrap()
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let mut path = self.inner.doc_dir.join(path.trim_left_matches('/'));
        if path.is_dir() {
            path.push("index.html");
        }
        path
    }
}

impl Service for ServiceContext {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = io::Error;
    type Future = Box<Future<Item = Response<Self::ResBody>, Error = Self::Error> + Send>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        if req.method() != Method::GET {
            return Box::new(future::ok(error_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "The request method is not GET",
            )));
        }

        if req.uri().path() == "/" || req.uri().path() == "/index.html" {
            return Box::new(future::ok(self.render_index()));
        }

        let path = self.resolve_path(req.uri().path());
        let content_type = guess_mime_type(&path);

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
                        Ok(error_response(StatusCode::NOT_FOUND, e))
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::PermissionDenied => {
                        Ok(error_response(StatusCode::FORBIDDEN, e))
                    }
                    Err(e) => Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, e)),
                }),
        )
    }
}

pub fn serve(ws: &Workspace) -> Fallible<()> {
    let target_dir = ws
        .config()
        .target_dir()?
        .map_or("./target".into(), Filesystem::into_path_unlocked);
    let doc_dir = target_dir.join("doc");
    let targets = ws
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
    let inner = Arc::new(Inner { doc_dir, targets });
    let new_service = move || {
        Ok::<_, io::Error>(ServiceContext {
            inner: inner.clone(),
        })
    };

    let addr = ([127, 0, 0, 1], 8000).into();
    let server = hyper::server::Server::bind(&addr)
        .serve(new_service)
        .map_err(|e| error!("server error: {}", e));

    ws.config()
        .shell()
        .status("Docserve", format!("Listening on http://{}", addr))?;
    hyper::rt::run(server);

    Ok(())
}

fn error_response(status: StatusCode, err: impl fmt::Display) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("cache-control", "no-cache")
        .header("connection", "close")
        .body(Body::from(err.to_string()))
        .unwrap()
}
