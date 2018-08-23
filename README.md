# `cargo-docserve`

A cargo subcommand for serving the artifacts from `cargo doc`.

## Status
Experimental

## Usage

Generates API documentation and start the HTTP server on `localhost:8000`

```shell-session
$ cargo docserve
```

Watches the modification in `src/`

```shell-session
$ cargo docserve --watch
```

## Install

```shell-session
$ cargo install --git https://github.com/ubnt-intrepid/cargo-docserve.git
```

## Testing

```shell-session
$ git clone https://github.com/ubnt-intrepid/cargo-docserve.git
$ cd cargo-docserve/
$ cargo run docserve [OPTIONS]
```

## TODOs

- [x] Detect filesystem notification and re-run `cargo doc`
- [ ] Cache-Control
- [ ] Bumps up `cargo` to 0.30.0

## License
MIT or Apache-2.0
