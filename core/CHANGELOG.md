# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/water-rs/nami/compare/core-v0.3.0...core-v0.3.1) - 2025-12-05

### Added

- Extend Signal trait to support Option and Result types with appropriate watch and get methods
- Introduce Distinct signal implementation to notify only on value changes
- Enhance Signal trait with Guard type and improve BindingMutGuard for efficient updates

### Fixed

- Remove outdated common helpers section from README for clarity

### Other

- Update dependencies and CI configuration for improved stability and performance
- Optimize Signal trait implementation and enhance SignalStream constructor for better performance
