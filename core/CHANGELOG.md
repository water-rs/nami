# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/water-rs/nami/compare/core-v0.4.0...core-v0.5.0) - 2026-10-15

### Added

- *(batch)* coalesce watcher notifications with nami::batch
- *(collection)* compose a sequence of changes with ChangeLog
- *(signal)* add Signal::with_ref, a borrowing read
- *(collection)* [**breaking**] rename Collection::len to count, drop is_empty, add with_range
- *(collection)* [**breaking**] List mutators take &mut self; warn on reentrant mutation
- *(collection)* [**breaking**] notify collection watchers only on actual changes
- *(collection)* [**breaking**] give Collection its own Id type; index ids for immutable sequences
- *(collection)* [**breaking**] pass collection watchers a borrowed slice
- *(collection)* [**breaking**] let collection watcher guards move their range in place
- *(collection)* [**breaking**] identify items through Identifiable instead of an identity parameter
- *(collection)* [**breaking**] identify items by id and report changes by id

### Fixed

- *(watcher)* [**breaking**] deliver reentrant notifications in order
- *(collection)* [**breaking**] keep diagnostics out of release builds
- *(collection)* flag duplicate ids across the whole snapshot
- *(s)* resolve the s! expansion through $crate

### Other

- *(collection)* compute ids lazily and match small middles linearly
- *(collection)* criterion benchmarks for the id diff
- *(collection)* [**breaking**] rename Collection::get/id to get_item/get_id
- *(collection)* [**breaking**] make Collection::id a required method
- *(watcher)* allocation-free notify path

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
