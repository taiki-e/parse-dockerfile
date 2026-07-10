# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org).

Releases may yanked if there is a security bug, a soundness bug, or a regression.

<!--
Note: In this file, do not use the hard wrap in the middle of a sentence for compatibility with GitHub comment style markdown rendering.
-->

## [Unreleased]

## [0.1.8] - 2026-07-10

- Specify the behavior of the iterator returned by `parse_iter` when `.next()` called after once `Some(Err(..))` returned, to return `None`. Previously, the behavior was unspecified, and the consistency of the results was not guaranteed. ([bbb6d15](https://github.com/taiki-e/parse-dockerfile/commit/bbb6d159489deaaafdb410b606e33b7e88453a91))

- Implement `FusedIterator` for the iterator returned by `parse_iter`. ([bbb6d15](https://github.com/taiki-e/parse-dockerfile/commit/bbb6d159489deaaafdb410b606e33b7e88453a91))

- Reduce memory usage in error case. ([8843b2e](https://github.com/taiki-e/parse-dockerfile/commit/8843b2e2421d37a8da81220f5269476551785dc8), [f54379e](https://github.com/taiki-e/parse-dockerfile/commit/f54379ee2da6dc43679e025f34dcdf6c14868e42), [63ccc01](https://github.com/taiki-e/parse-dockerfile/commit/63ccc0120ab7608d7cc893b3599c3f29b5c00d29), [b2013a8](https://github.com/taiki-e/parse-dockerfile/commit/b2013a8f320f8453aa21756699ed323813a25b77))

## [0.1.7] - 2026-06-06

- Fix bug in heredoc parsing.

- Fix bug in `\r`/`\f`/`\v` handling.

- Limit memory usage for large input.

- Relax the minimum supported Rust version (MSRV) from Rust 1.80 to Rust 1.71.

- Documentation improvements.

## [0.1.6] - 2026-05-24

- Fix bug in JSON array parsing.

## [0.1.5] - 2026-03-20

- Publish [artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations).

## [0.1.4] - 2026-02-11

- Diagnostics improvements.

## [0.1.3] - 2026-01-06

- Enable [release immutability](https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases).

## [0.1.2] - 2025-09-07

- Distribute prebuilt binaries for powerpc64le/riscv64gc/s390x Linux.

## [0.1.1] - 2025-02-06

- Documentation improvements.

## [0.1.0] - 2025-02-01

Initial release

[Unreleased]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/taiki-e/parse-dockerfile/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/taiki-e/parse-dockerfile/releases/tag/v0.1.0
