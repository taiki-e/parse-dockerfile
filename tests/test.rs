// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::path::Path;

use fs_err as fs;
use parse_dockerfile::*;
use test_helper::git::assert_diff;

fn fixtures_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

static ALL_INST: &[&str] = &[
    "ADD",
    "ARG",
    "CMD",
    "COPY",
    "ENTRYPOINT",
    "ENV",
    "EXPOSE",
    "FROM",
    "HEALTHCHECK CMD",
    "HEALTHCHECK NONE",
    "LABEL",
    "MAINTAINER",
    "RUN",
    "SHELL",
    "STOPSIGNAL",
    "USER",
    "VOLUME",
    "WORKDIR",
    "ONBUILD ADD",
    "ONBUILD ARG",
    "ONBUILD CMD",
    "ONBUILD COPY",
    "ONBUILD ENTRYPOINT",
    "ONBUILD ENV",
    "ONBUILD EXPOSE",
    "ONBUILD FROM",
    "ONBUILD HEALTHCHECK CMD",
    "ONBUILD HEALTHCHECK NONE",
    "ONBUILD LABEL",
    "ONBUILD MAINTAINER",
    "ONBUILD RUN",
    "ONBUILD SHELL",
    "ONBUILD STOPSIGNAL",
    "ONBUILD USER",
    "ONBUILD VOLUME",
    "ONBUILD WORKDIR",
];

#[test]
fn failure() {
    let tests: &[(&str, &str)] = &[
        (
            "FROM a
USER <<INVALID
INVALID
",
            "unknown instruction 'INVALID' at line 3 column 1",
        ),
        // TODO: shouldn't fail
        (
            "FROM a
RUN 3<<EMPTY2
EMPTY2
",
            "unknown instruction 'EMPTY2' at line 3 column 1",
        ),
        // TODO: shouldn't fail
        (
            "FROM a
RUN <<eo'f'
echo foo
eof
",
            "expected end of here-document (eo), but reached eof at line 5 column 1",
        ),
        // TODO: shouldn't fail
        (
            "FROM a
RUN <<eo\'f
echo foo
eo'f
",
            "expected end of here-document (eo), but reached eof at line 5 column 1",
        ),
        // TODO: shouldn't fail
        (
            "FROM a
RUN <<'e'o\'f
echo foo
eo'f
",
            "expected end of here-document (e), but reached eof at line 5 column 1",
        ),
        // TODO: shouldn't fail
        (
            "FROM a
RUN <<'one two'
echo bar
one two
",
            "expected end of quoted string ('), but found ' ' at line 2 column 11",
        ),
        // TODO: shouldn't fail
        (
            "FROM a
RUN <<$EOF
$EOF
",
            "unknown instruction '$EOF' at line 3 column 1",
        ),
        (
            "FROM a
SHELL",
            "expected JSON array at line 2 column 6",
        ),
        (
            "FROM a
HEALTHCHECK NONE a",
            "HEALTHCHECK NONE does not accept arguments at line 2 column 18",
        ),
        (
            "FROM a
HEALTHCHECK",
            "expected CMD or NONE at line 2 column 12",
        ),
        (
            "FROM a
ONBUILD",
            "expected instruction after ONBUILD at line 2 column 1",
        ),
        (
            "FROM a
ONBUILD ONBUILD",
            "ONBUILD ONBUILD is not allowed at line 2 column 9",
        ),
    ];
    for &(test, expected_err) in tests {
        assert_eq!(parse(test).unwrap_err().to_string(), expected_err);
    }

    for &inst in ALL_INST {
        fn check(inst: &str, onbuild: bool, json: bool) {
            let m = match (inst, json) {
                ("CMD" | "HEALTHCHECK NONE", _)
                | ("HEALTHCHECK CMD" | "RUN", false)
                | ("VOLUME", true) => "",
                ("MAINTAINER" | "STOPSIGNAL" | "USER" | "WORKDIR", _) => "exactly one argument",
                ("ADD" | "COPY", _) => "at least two arguments",
                _ => "at least one argument",
            };
            let mut text = format!(
                "FROM a
{}{inst}",
                if onbuild { "ONBUILD " } else { "" }
            );
            if json {
                text.push_str(" []");
            }
            if m.is_empty() {
                parse(&text).unwrap();
                text.push(' ');
                parse(&text).unwrap();
            } else {
                // TODO: show "ONBUILD "
                let err = format!(
                    "{inst} instruction requires {m} at line 2 column {}",
                    if onbuild { 9 } else { 1 }
                );
                assert_eq!(parse(&text).unwrap_err().to_string(), err);
                text.push(' ');
                assert_eq!(parse(&text).unwrap_err().to_string(), err);
                if json {
                    text.pop();
                    text.pop();
                    text.push('"');
                }
                text.push('a');
                if json {
                    text.push_str("\"]");
                }
                if m == "at least two arguments" {
                    assert_eq!(parse(&text).unwrap_err().to_string(), err);
                    text.push(' ');
                    assert_eq!(parse(&text).unwrap_err().to_string(), err);
                } else {
                    parse(&text).unwrap();
                    text.push(' ');
                    parse(&text).unwrap();
                }
                // TODO
                // if m == "exactly one argument" {
                //     assert_eq!(parse(&text).unwrap_err().to_string(), err);
                //     text.push(' ');
                //     assert_eq!(parse(&text).unwrap_err().to_string(), err);
                // } else {
                if inst == "FROM" {
                    return;
                }
                if json {
                    text.pop();
                    text.pop();
                    text.push_str(",\"");
                }
                text.push('b');
                if json {
                    text.push_str("\"]");
                }
                parse(&text).unwrap();
                text.push(' ');
                parse(&text).unwrap();
                // }
            }
        }
        let (inst, onbuild) = if let Some(inst) = inst.strip_prefix("ONBUILD ") {
            (inst, true)
        } else {
            (inst, false)
        };
        check(inst, onbuild, matches!(inst, "SHELL"));
        if matches!(
            inst,
            "ADD" | "CMD" | "COPY" | "ENTRYPOINT" | "HEALTHCHECK CMD" | "RUN" | "VOLUME"
        ) {
            check(inst, onbuild, true);
        }
    }
}

// Regression tests for bugs caught by fuzzing.
#[test]
fn fuzz() {
    let tests: &[(&str, Result<usize, &str>)] = &[
        ("", Err("expected at least one FROM instruction")),
        (
            str::from_utf8(&[
                35, 32, 69, 83, 99, 65, 80, 69, 32, 61, 0, 0, 2, 30, 0, 0, 0, 0, 0, 10, 9, 9, 35,
            ])
            .unwrap(),
            Err("invalid escape '\0\0\u{2}\u{1e}\0\0\0\0\0' at line 1 column 11"),
        ),
        (
            str::from_utf8(&[
                97, 68, 68, 92, 13, 9, 68, 92, 13, 9, 10, 9, 92, 13, 9, 92, 13, 9, 10, 9, 10, 9, 9,
                92, 13, 9, 92, 13, 13, 9, 10, 9, 92, 13, 9, 92, 13, 9, 10, 9, 10, 9, 9, 92, 13, 9,
                92, 13, 9, 92, 13, 9, 92, 13, 9, 92, 13, 9, 10, 9, 10, 9, 9, 9, 92, 13, 9, 92, 13,
                9, 92, 13, 9, 10, 9, 10, 9, 9, 92, 13, 9, 92, 13, 9, 9, 10, 9, 92, 13, 9, 92, 13,
                9, 10, 9, 10, 9, 9, 92, 13, 9, 92, 13, 13, 9, 10, 9, 92, 13, 9, 92, 13, 9, 10, 9,
                10, 9, 9, 92, 13, 9, 92, 13, 9, 92, 13, 9, 92, 13, 9, 92, 13, 9, 10, 9, 10, 9, 9,
                9, 92, 13, 9, 92, 13, 9, 92, 13, 9, 10, 9, 10, 9, 9, 92, 13, 9, 92, 13, 9, 9, 13,
                9,
            ])
            .unwrap(),
            Err("aDD instruction requires at least two arguments at line 1 column 1"),
        ),
        (
            str::from_utf8(&[
                97, 68, 68, 9, 91, 34, 91, 34, 44, 34, 25, 53, 53, 53, 53, 53, 53, 53, 53, 53, 53,
                53, 53, 53, 91, 0, 0, 0, 53, 53, 53, 53, 0, 0, 0, 0, 0, 0, 39, 0, 0, 97, 68, 68,
                44, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 0, 0, 101, 101, 101, 101, 101, 101, 0, 0, 0,
                0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0,
                0, 0, 0, 44, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 20, 0, 25, 96, 0, 14, 0, 0, 0,
                101, 101, 101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15,
                101, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44,
                34, 101, 101, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 68, 9, 91, 34, 44,
                34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0, 0, 97, 68, 68, 9, 91, 34, 44,
                34, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13,
                9, 92, 13, 13, 9, 35, 15, 101, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25,
                0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96,
                0, 14, 0, 0, 0, 101, 101, 101, 101, 101, 101, 101, 0, 0, 64, 0, 91, 34, 44, 101,
                78, 84, 0, 14, 0, 0, 9, 91, 34, 91, 34, 44, 34, 25, 50, 55, 55, 55, 55, 55, 55, 55,
                55, 55, 55, 55, 55, 91, 0, 0, 0, 53, 53, 53, 53, 0, 0, 0, 0, 0, 0, 39, 0, 0, 97,
                68, 68, 44, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 0, 0, 101, 101, 101, 101, 101, 101,
                0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101,
                101, 0, 0, 0, 0, 44, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 20, 0, 25, 96, 0, 14,
                0, 0, 0, 101, 101, 101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9,
                35, 15, 101, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91,
                34, 14, 0, 0, 0, 101, 101, 101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13,
                13, 9, 35, 15, 101, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0,
                9, 91, 34, 44, 34, 101, 101, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0,
                0, 101, 101, 101, 101, 101, 101, 101, 0, 0, 64, 0, 91, 34, 44, 101, 78, 84, 0, 14,
                0, 0, 9, 91, 34, 91, 34, 44, 34, 25, 50, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55,
                55, 55, 91, 0, 0, 0, 53, 53, 53, 53, 0, 0, 0, 0, 0, 0, 39, 0, 0, 97, 68, 68, 44, 9,
                91, 34, 44, 34, 25, 0, 0, 0, 0, 0, 0, 101, 101, 101, 101, 101, 101, 0, 0, 0, 0, 97,
                68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 53, 53, 53, 53, 53, 53, 53, 53, 53, 53, 53,
                91, 0, 0, 0, 53, 53, 53, 53, 0, 0, 0, 0, 0, 0, 39, 0, 0, 97, 68, 68, 44, 9, 91, 34,
                44, 34, 25, 0, 0, 0, 0, 0, 0, 101, 101, 101, 101, 101, 101, 0, 0, 0, 0, 97, 68, 68,
                9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0, 0, 0, 0, 44, 0,
                0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 20, 0, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101,
                101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 101, 101, 0, 0,
                0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101,
                0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0,
                0, 9, 91, 34, 44, 34, 101, 101, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14,
                0, 0, 0, 101, 101, 101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9,
                35, 15, 101, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91,
                34, 44, 34, 101, 101, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0, 0,
                101, 101, 101, 101, 101, 101, 101, 0, 0, 64, 0, 91, 34, 44, 101, 78, 84, 0, 14, 0,
                0, 9, 91, 34, 91, 34, 44, 34, 25, 50, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55,
                55, 91, 0, 0, 0, 53, 53, 53, 53, 0, 0, 0, 0, 0, 0, 39, 0, 0, 97, 68, 68, 44, 9, 91,
                34, 44, 34, 25, 0, 0, 0, 0, 0, 0, 101, 101, 101, 101, 101, 101, 0, 0, 0, 0, 97, 68,
                68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0, 0, 0, 0, 44,
                0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 20, 0, 25, 96, 0, 14, 0, 0, 0, 101, 101,
                101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 101, 101, 0,
                0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 14, 0, 0, 0,
                101, 101, 101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15,
                101, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44,
                34, 101, 101, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0, 0, 101, 101,
                101, 101, 101, 101, 101, 0, 0, 64, 0, 91, 34, 44, 101, 78, 84, 0, 14, 0, 0, 9, 91,
                34, 91, 34, 44, 34, 25, 50, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 91, 0,
                0, 0, 53, 53, 53, 53, 0, 0, 0, 0, 0, 0, 39, 0, 0, 97, 68, 68, 44, 9, 91, 34, 44,
                34, 25, 0, 0, 0, 0, 0, 0, 101, 101, 101, 101, 101, 101, 0, 0, 0, 0, 97, 68, 68, 9,
                91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0, 0, 0, 0, 44, 0, 0,
                0, 97, 68, 68, 9, 91, 34, 44, 34, 20, 0, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101,
                101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 101, 101, 0, 0,
                0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101,
                0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101, 101,
                101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 101, 101, 0, 0, 0, 0,
                97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101, 0, 97,
                68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101, 101, 101, 101,
                101, 0, 0, 64, 0, 91, 34, 44, 101, 78, 84, 0, 14, 0, 0, 0, 101, 101, 101, 101, 101,
                9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 97, 101, 0, 0, 0, 0, 97,
                68, 68, 9, 91, 34, 44, 34, 105, 25, 0, 0, 0, 101, 101, 101, 101, 101, 9, 92, 13, 9,
                92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 97, 101, 0, 0, 0, 0, 97, 68, 68, 9, 91,
                34, 44, 34, 105, 25, 0, 0, 0, 0, 9, 0, 9, 91, 34, 44, 34, 101, 101, 0, 0, 0, 0, 44,
                0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 20, 0, 25, 96, 0, 14, 0, 0, 0, 101, 101,
                101, 101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 101, 101, 0,
                0, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101,
                101, 0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101,
                101, 101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 101, 101, 0, 0,
                0, 0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 0, 0, 0, 0, 9, 91, 34, 44, 34, 101, 101,
                0, 97, 68, 68, 9, 91, 34, 44, 34, 25, 96, 0, 14, 0, 0, 0, 101, 101, 101, 101, 101,
                101, 101, 0, 0, 64, 0, 91, 34, 44, 101, 78, 84, 0, 14, 0, 0, 0, 101, 101, 101, 101,
                101, 9, 92, 13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 97, 101, 0, 0, 0, 0,
                97, 68, 68, 9, 91, 34, 44, 34, 105, 25, 0, 0, 0, 101, 101, 101, 101, 101, 9, 92,
                13, 9, 92, 13, 13, 13, 9, 92, 13, 13, 9, 35, 15, 97, 101, 0, 0, 0, 0, 97, 68, 68,
                9, 91, 34, 44, 34, 105, 25, 0, 0, 0, 0, 9, 91,
            ])
            .unwrap(),
            Err("expected FROM at line 1 column 1"),
        ),
        (
            str::from_utf8(&[115, 72, 69, 76, 76, 9]).unwrap(),
            Err("expected JSON array at line 1 column 7"),
        ),
    ];
    for &(test, expected_len) in tests {
        let res = parse(test);
        match expected_len {
            Ok(expected_len) => {
                assert_eq!(res.unwrap().instructions.len(), expected_len);
            }
            Err(s) => {
                assert_eq!(res.unwrap_err().to_string(), s);
            }
        }
    }
}

#[test]
fn dump() {
    let fixtures_dir = &fixtures_dir();
    for e in fs::read_dir(fixtures_dir).unwrap() {
        let p = &e.unwrap().path();
        if p.is_dir() {
            continue;
        }
        let text = &fs::read_to_string(p).unwrap();
        let dockerfile = parse(text).unwrap();
        for r in parse_dockerfile::parse_iter(text).unwrap() {
            r.unwrap();
        }
        let dump = serde_json::to_vec_pretty(&dockerfile).unwrap();
        let mut dump_path = fixtures_dir.join("dump").join(p.file_name().unwrap());
        dump_path.as_mut_os_string().push(".dump.json"); // TODO: Use .add_extension("dump.json") once stabilized.
        assert_diff(dump_path, dump);
    }
}
