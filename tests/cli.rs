// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(feature = "default")]
#![cfg(not(miri))] // Miri doesn't support pipe2 (inside std::process::Command::output)

use std::{ffi::OsStr, path::Path, process::Command};

use test_helper::cli::{ChildExt as _, CommandExt as _};

fn parse_dockerfile<O: AsRef<OsStr>>(args: impl AsRef<[O]>) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_parse-dockerfile"));
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd.args(args.as_ref());
    cmd
}

#[test]
fn failure() {
    parse_dockerfile([] as [&str; 0])
        .assert_failure()
        .stderr_contains("no dockerfile path specified");

    parse_dockerfile(["a", "b"]).assert_failure().stderr_contains(r#"unexpected argument "b""#);

    parse_dockerfile(["-"]).spawn_with_stdin("\n").assert_failure().stderr_contains(
        "error: expected at least one FROM instruction in dockerfile (standard input)",
    );

    parse_dockerfile(["-"])
        .spawn_with_stdin([b'f', b'o', 0x80, b'o'])
        .assert_failure()
        .stderr_contains(
            "error: failed to read from standard input: stream did not contain valid UTF-8",
        );
}

#[test]
fn help() {
    let short = parse_dockerfile(["-h"]).assert_success();
    let long = parse_dockerfile(["--help"]).assert_success();
    assert_eq!(short.stdout, long.stdout);
}

#[test]
fn version() {
    let expected = &format!("parse-dockerfile {}", env!("CARGO_PKG_VERSION"));
    parse_dockerfile(["-V"]).assert_success().stdout_eq(expected);
    parse_dockerfile(["--version"]).assert_success().stdout_eq(expected);
}

#[test]
fn update_readme() {
    let new = parse_dockerfile(["--help"]).assert_success().stdout;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let command = "parse-dockerfile --help";
    test_helper::doc::sync_command_output_to_markdown(path, "readme-long-help", command, new);
}
