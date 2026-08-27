# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.1](https://github.com/water-rs/nami/compare/v0.11.0...v0.11.1) - 2026-08-27

### Fixed

- [**breaking**] give the derive crate its own breaking version

## [0.11.0](https://github.com/water-rs/nami/compare/v0.10.1...v0.11.0) - 2026-08-25

### Added

- *(core)* Observe the reactive graph behind an opt-in feature.
- Add atomic list replacement and signal-backed reactive collections.
- Implement `Signal` for fixed-size arrays.

### Changed

- **Breaking:** Rename the string signal helpers so they no longer shadow slice methods.
- Upgrade the executor and reactive dependency stack.
- Remove renderer-owned local-binding hooks in favor of explicit signal state.

### Fixed

- Keep graph observation safe when a signal is dropped during thread teardown.
- Prevent `Distinct::watch` from double-borrowing its watcher state.

## [0.10.0](https://github.com/water-rs/nami/compare/v0.9.1...v0.10.0) - 2026-01-22

### Other

- Add usize and isize bindings
- Use macro to add typed Binding constructors

## [0.9.1](https://github.com/water-rs/nami/compare/v0.9.0...v0.9.1) - 2025-12-05

### Added

- Extend Signal trait to support Option and Result types with appropriate watch and get methods
- Introduce Distinct signal implementation to notify only on value changes

### Other

- Remove unused dev-dependency 'nami' from Cargo.toml
- Update git release name format in release configuration
- Remove changelog configuration from release settings
- Remove outdated file update configuration from release settings
- Update release configuration by removing default commit message template
- Remove unmaintained  dependency by updating nami-derive dev-dependency
- Add rust-cache action to CI workflow for improved build caching
- Update CI workflow to include lockfile generation before security audit
- Update CI workflow permissions for enhanced security and access control
- Update dependencies and CI configuration for improved stability and performance
- Optimize Signal trait implementation and enhance SignalStream constructor for better performance
- Add release configuration and update CI workflows for improved release management
- Revise SignalStream implementation to utilize async channels and improve polling logic
