# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2020-03-02
### Added

- Support for distributed tracing behind the `distributed_tracing` feature flag. See the `distribued_trace` method of `Segment` for details (from [@bobbyrward](https://github.com/bobbyrward)).
- Support for async custom segments behind the `async` feature flag. See the `Segmented` trait for details (from [@bobbyrward](https://github.com/bobbyrward)).
- Ability to change transaction names using `Transaction::name` (from [@bobbyrward](https://github.com/bobbyrward)).
- An `App` can now be created fluidly using the `AppBuilder`. Many more settings can now be configured, including various recording thresholds, SQL recording obfuscation and event spans. See the new methods on the `AppBuilder` struct for details (from [@bobbyrward](https://github.com/bobbyrward)).

### Changed

- Upgrade to use `newrelic-sys` v0.2.0 (from [@bobbyrward](https://github.com/bobbyrward)).
- `ExternalParams` and `DatastoreParams` are now `Send` / `Sync` to enable use in async segments.

## [0.2.1] - 2019-11-12
### Removed

- Removed trivial dependency on `derive_more`.

## [0.2.0] - 2019-05-31
### Added

- First version of the crate using this repository and the New Relic C SDK.

[Unreleased]: https://github.com/sd2k/newrelic/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/sd2k/newrelic/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/sd2k/newrelic/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/sd2k/newrelic/releases/tag/v0.2.0
