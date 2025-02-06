# parse-dockerfile

[![crates.io](https://img.shields.io/crates/v/parse-dockerfile?style=flat-square&logo=rust)](https://crates.io/crates/parse-dockerfile)
[![docs.rs](https://img.shields.io/badge/docs.rs-parse--dockerfile-blue?style=flat-square&logo=docs.rs)](https://docs.rs/parse-dockerfile)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)
[![msrv](https://img.shields.io/badge/msrv-1.80-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![github actions](https://img.shields.io/github/actions/workflow/status/taiki-e/parse-dockerfile/ci.yml?branch=main&style=flat-square&logo=github)](https://github.com/taiki-e/parse-dockerfile/actions)

Dockerfile parser, written in Rust.

- [Usage (CLI)](#usage-cli)
  - [Installation](#installation)
- [Usage (Library)](#usage-library)
- [License](#license)

## Usage (CLI)

`parse-dockerfile` command parses dockerfile and outputs a JSON representation
of all instructions in dockerfile.

<details>
<summary>Complete list of options (click to show)</summary>

<!-- readme-long-help:start -->
```console
$ parse-dockerfile --help
parse-dockerfile

Parse a dockerfile and output a JSON representation of all instructions in dockerfile.

USAGE:
    parse-dockerfile [OPTIONS] <PATH>

ARGS:
    <PATH>       Path to the dockerfile (use '-' for standard input)

OPTIONS:
    -h, --help                        Print help information
    -V, --version                     Print version information
```
<!-- readme-long-help:end -->

</details>

<!-- omit in toc -->
### Examples

```console
$ cat Dockerfile
ARG UBUNTU_VERSION=latest

FROM ubuntu:${UBUNTU_VERSION}
RUN echo

$ parse-dockerfile Dockerfile | jq
{
  "parser_directives": {
    "syntax": null,
    "escape": null,
    "check": null
  },
  "instructions": [
    {
      "kind": "ARG",
      "arg": {
        "span": {
          "start": 0,
          "end": 3
        }
      },
      "arguments": {
        "span": {
          "start": 4,
          "end": 18
        },
        "value": "UBUNTU_VERSION=latest"
      }
    },
    {
      "kind": "FROM",
      "from": {
        "span": {
          "start": 20,
          "end": 24
        }
      },
      "options": [],
      "image": {
        "span": {
          "start": 25,
          "end": 49
        },
        "value": "ubuntu:${UBUNTU_VERSION}"
      },
      "as_": null
    },
    {
      "kind": "RUN",
      "run": {
        "span": {
          "start": 50,
          "end": 53
        }
      },
      "options": [],
      "arguments": {
        "shell": {
          "span": {
            "start": 54,
            "end": 58
          },
          "value": "echo"
        }
      },
      "here_docs": []
    }
  ]
}
```

### Installation

<!-- omit in toc -->
#### From source

```sh
cargo +stable install parse-dockerfile --locked
```

<!-- omit in toc -->
#### From prebuilt binaries

You can download prebuilt binaries from the [Release page](https://github.com/taiki-e/parse-dockerfile/releases).
Prebuilt binaries are available for macOS, Linux (gnu and musl), Windows (static executable), FreeBSD, and illumos.

<details>
<summary>Example of script to install from the Release page (click to show)</summary>

```sh
# Get host target
host=$(rustc -vV | grep '^host:' | cut -d' ' -f2)
# Download binary and install to $HOME/.cargo/bin
curl --proto '=https' --tlsv1.2 -fsSL "https://github.com/taiki-e/parse-dockerfile/releases/latest/download/parse-dockerfile-$host.tar.gz" \
  | tar xzf - -C "$HOME/.cargo/bin"
```

</details>

<!-- omit in toc -->
#### Via Homebrew

You can install parse-dockerfile from the [Homebrew tap maintained by us](https://github.com/taiki-e/homebrew-tap/blob/HEAD/Formula/parse-dockerfile.rb) (x86_64/AArch64 macOS, x86_64/AArch64 Linux):

```sh
brew install taiki-e/tap/parse-dockerfile
```

<!-- omit in toc -->
#### Via Scoop (Windows)

You can install parse-dockerfile from the [Scoop bucket maintained by us](https://github.com/taiki-e/scoop-bucket/blob/HEAD/bucket/parse-dockerfile.json):

```sh
scoop bucket add taiki-e https://github.com/taiki-e/scoop-bucket
scoop install parse-dockerfile
```

<!-- omit in toc -->
#### Via cargo-binstall

You can install parse-dockerfile using [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall parse-dockerfile
```

<!-- omit in toc -->
#### On GitHub Actions

You can use [taiki-e/install-action](https://github.com/taiki-e/install-action) to install prebuilt binaries on Linux, macOS, and Windows.
This makes the installation faster and may avoid the impact of [problems caused by upstream changes](https://github.com/tokio-rs/bytes/issues/506).

```yaml
- uses: taiki-e/install-action@parse-dockerfile
```

## Usage (Library)

<!-- tidy:sync-markdown-to-rustdoc:start:src/lib.rs -->

To use this crate as a library, add this to your `Cargo.toml`:

```toml
[dependencies]
parse-dockerfile = { version = "0.1", default-features = false }
```

> [!NOTE]
> We recommend disabling default features because they enable CLI-related
> dependencies which the library part does not use.

<!-- omit in toc -->
### Examples

```rust
use parse_dockerfile::{parse, Instruction};

let text = r#"
ARG UBUNTU_VERSION=latest

FROM ubuntu:${UBUNTU_VERSION}
RUN echo
"#;

let dockerfile = parse(text).unwrap();

// Iterate over all instructions.
let mut instructions = dockerfile.instructions.iter();
assert!(matches!(instructions.next(), Some(Instruction::Arg(..))));
assert!(matches!(instructions.next(), Some(Instruction::From(..))));
assert!(matches!(instructions.next(), Some(Instruction::Run(..))));
assert!(matches!(instructions.next(), None));

// Iterate over global args.
let mut global_args = dockerfile.global_args();
let global_arg1 = global_args.next().unwrap();
assert_eq!(global_arg1.arguments.value, "UBUNTU_VERSION=latest");
assert!(matches!(global_args.next(), None));

// Iterate over stages.
let mut stages = dockerfile.stages();
let stage1 = stages.next().unwrap();
assert_eq!(stage1.from.image.value, "ubuntu:${UBUNTU_VERSION}");
let mut stage1_instructions = stage1.instructions.iter();
assert!(matches!(stage1_instructions.next(), Some(Instruction::Run(..))));
assert!(matches!(stage1_instructions.next(), None));
assert!(matches!(stages.next(), None));
```

<!-- tidy:sync-markdown-to-rustdoc:ignore:start -->

See [documentation](https://docs.rs/parse-dockerfile) for more information on
`parse-dockerfile` as a library.

<!-- tidy:sync-markdown-to-rustdoc:ignore:end -->

<!-- omit in toc -->
### Optional features

- **`serde`** — Implements [`serde::Serialize`] trait for parse-dockerfile types.

[`serde::Serialize`]: https://docs.rs/serde/latest/serde/trait.Serialize.html

<!-- tidy:sync-markdown-to-rustdoc:end -->

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
