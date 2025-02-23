// SPDX-License-Identifier: Apache-2.0 OR MIT

#![cfg(not(miri))] // Miri doesn't support pipe2 (inside std::process::Command::output)

use std::{env, fmt::Write as _, path::Path, process::Command};

use fs_err as fs;
use test_helper::git::{assert_diff, ls_files};

#[test]
#[cfg_attr(windows, ignore)] // git submodule eol issue
fn test() {
    let repos = [("moby", 26), ("buildkit", 58), ("buildah", 307)];

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let external_dir = &manifest_dir.join("tests/external");
    for (base, expected) in repos {
        assert!(expected != 0);
        let mut all = Vec::with_capacity(expected);
        let root_dir = &external_dir.join(base);
        let dump_dir = &external_dir.join("dump").join(base);
        let mut num_dockerfiles = 0;
        loop {
            for (rel, abs) in ls_files(root_dir, &["*Dockerfile*"]) {
                let mut expected_err = None;
                match base {
                    "moby" => {
                        if rel.starts_with("contrib/syntax") {
                            // not dockerfile
                            continue;
                        }
                    }
                    "buildkit" => {
                        if rel.ends_with("/empty_dockerfile/Dockerfile")
                            || rel.ends_with("/only_comments/Dockerfile")
                        {
                            expected_err = Some("expected at least one FROM instruction");
                        } else if rel.ends_with("/shykes-nested-json/Dockerfile")
                            || rel.ends_with(
                                "jeztah-invalid-json-json-inside-string-double/Dockerfile",
                            )
                            || rel.ends_with("jeztah-invalid-json-json-inside-string/Dockerfile")
                            || rel.ends_with("jeztah-invalid-json-single-quotes/Dockerfile")
                            || rel.ends_with("jeztah-invalid-json-unterminated-bracket/Dockerfile")
                            || rel.ends_with("jeztah-invalid-json-unterminated-string/Dockerfile")
                            || rel == "frontend/dockerfile/parser/testfiles/json/Dockerfile"
                        {
                            // TODO: test with json parser
                            expected_err = Some("expected FROM");
                        } else if rel == "frontend/dockerfile/parser/testfiles/health/Dockerfile" {
                            // odd syntax
                            expected_err = Some("expected CMD or NONE");
                        }
                    }
                    "buildah" => {
                        if rel.ends_with(".dockerignore") {
                            // not dockerfile
                            continue;
                        }
                        if rel.ends_with(".nofrom") {
                            expected_err = Some("expected FROM");
                        } else if rel.ends_with("/unrecognized/Dockerfile") {
                            expected_err = Some("unknown instruction 'BOGUS'");
                        } else if rel.ends_with("/Dockerfilefromarg") {
                            expected_err = Some("duplicate name 'final'");
                        }
                    }
                    _ => {}
                }

                eprintln!();
                eprintln!("parsing {:?}", abs.strip_prefix(manifest_dir).unwrap());
                num_dockerfiles += 1;
                let text = &*fs::read_to_string(abs).unwrap();
                let res = parse_dockerfile::parse(text);
                if let Some(expected_err) = expected_err {
                    let err = &*res.unwrap_err().to_string();
                    assert!(err.contains(expected_err), "expected '{expected_err}' actual '{err}'");
                    continue;
                }
                let dockerfile = res.unwrap();
                parse_dockerfile::parse_iter(text).unwrap().for_each(|r| drop(r.unwrap()));
                let dump = serde_json::to_vec_pretty(&dockerfile).unwrap();
                let mut dump_path = external_dir.join("dump").join(base).join(&rel);
                let file_name = dump_path.file_name().unwrap().to_str().unwrap();
                let file_name = format!("_{file_name}.dump.json");
                dump_path.pop();
                dump_path.push(file_name);
                assert_diff(dump_path, dump);
                all.push((text.len(), rel));
            }
            if num_dockerfiles != 0 {
                break;
            }
            assert!(
                Command::new("git")
                    .current_dir(manifest_dir)
                    .args(["submodule", "update", "--init", "--recursive",])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        assert_eq!(num_dockerfiles, expected);
        all.sort_unstable();
        all.reverse();
        let mut out = String::new();
        for (size, path) in all {
            let _ = write!(out, "{size} ");
            out.push_str(&path);
            out.push('\n');
        }
        assert_diff(external_dir.join(format!("{base}-size.txt")), out);
        for file in ["LICENSE", "NOTICE"] {
            let upstream = root_dir.join(file);
            if upstream.exists() {
                assert_diff(dump_dir.join(file), fs::read(upstream).unwrap());
            }
        }
    }
}
