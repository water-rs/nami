# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/water-rs/nami/compare/core-v0.3.3...core-v0.4.0) - 2026-10-05

### Added

- *(collection)* [**breaking**] notifications carry a typed CollectionChange
- implement constant Signal for kurbo types behind a kurbo feature
- [**breaking**] rename BindingMailbox::get/get_as to snapshot/snapshot_as
- [**breaking**] rename Signal::get to Signal::snapshot
- add on_unimplemented diagnostics to Signal, Collection, IntoSignal and IntoComputed ([#18](https://github.com/water-rs/nami/pull/18))

### Fixed

- *(ci)* repair lint checks and trusted publishing
- *(async_signal)* keep debounce/throttle upstream alive for the watch's lifetime
- *(watcher)* drop cancelled watcher after releasing inner borrow

## [0.3.3](https://github.com/water-rs/nami/compare/core-v0.3.2...core-v0.3.3) - 2026-08-25

### Added

- Add opt-in reactive graph observation and stable signal identities.
- Implement `Signal` for fixed-size arrays and reactive tuples.

### Fixed

- Keep graph observation safe when a signal is dropped during thread teardown.

## [0.3.2](https://github.com/water-rs/nami/compare/core-v0.3.1...core-v0.3.2) - 2026-01-22

### Other

- release
- release

## [0.3.1](https://github.com/water-rs/nami/compare/core-v0.3.0...core-v0.3.1) - 2026-01-22

### Other

- release
