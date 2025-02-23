// SPDX-License-Identifier: Apache-2.0 OR MIT

/*

Just for reference purposes, leave results of local benchmarks.


## Versions

- parse-dockerfile 0.1.0
  - smallvec 1.13.2
- dockerfile-parser 0.9.0
  - enquote 1.1.0
  - pest 2.7.15
  - regex 1.11.1


## Apple M1 Pro (MacBook Pro 2021, macOS 15.2)

```console
$ sysctl machdep.cpu.brand_string
machdep.cpu.brand_string: Apple M1 Pro

$ rustc -vV
rustc 1.86.0-nightly (854f22563 2025-01-31)
binary: rustc
commit-hash: 854f22563c8daf92709fae18ee6aed52953835cd
commit-date: 2025-01-31
host: aarch64-apple-darwin
release: 1.86.0-nightly
LLVM version: 19.1.7

$ cargo bench -p bench
<cargo log omitted>

parse_dockerfile_unix_28.3kb/parse_pull
                        time:   [34.804 µs 34.844 µs 34.888 µs]
                        thrpt:  [774.03 MiB/s 775.00 MiB/s 775.90 MiB/s]
Found 4 outliers among 100 measurements (4.00%)
  1 (1.00%) high mild
  3 (3.00%) high severe
parse_dockerfile_unix_28.3kb/parse_dom
                        time:   [43.190 µs 43.236 µs 43.283 µs]
                        thrpt:  [623.91 MiB/s 624.57 MiB/s 625.24 MiB/s]
Found 8 outliers among 100 measurements (8.00%)
  6 (6.00%) high mild
  2 (2.00%) high severe

parse_dockerfile_unix_17.6kb/parse_pull
                        time:   [23.002 µs 23.026 µs 23.052 µs]
                        thrpt:  [729.17 MiB/s 729.99 MiB/s 730.73 MiB/s]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high severe
parse_dockerfile_unix_17.6kb/parse_dom
                        time:   [29.422 µs 29.450 µs 29.479 µs]
                        thrpt:  [570.19 MiB/s 570.75 MiB/s 571.30 MiB/s]
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) high mild
  1 (1.00%) high severe

parse_dockerfile_windows_13.6kb/parse_pull
                        time:   [11.195 µs 11.239 µs 11.323 µs]
                        thrpt:  [1.1245 GiB/s 1.1329 GiB/s 1.1374 GiB/s]
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe
parse_dockerfile_windows_13.6kb/parse_dom
                        time:   [11.345 µs 11.355 µs 11.366 µs]
                        thrpt:  [1.1203 GiB/s 1.1214 GiB/s 1.1224 GiB/s]
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) high mild
  6 (6.00%) high severe

parse_dockerfile_unix_4.7kb_no_here_doc/parse_pull
                        time:   [4.8217 µs 4.8641 µs 4.9532 µs]
                        thrpt:  [911.85 MiB/s 928.56 MiB/s 936.72 MiB/s]
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
parse_dockerfile_unix_4.7kb_no_here_doc/parse_dom
                        time:   [5.3115 µs 5.3158 µs 5.3197 µs]
                        thrpt:  [849.03 MiB/s 849.66 MiB/s 850.34 MiB/s]
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) high mild
  1 (1.00%) high severe

parse_dockerfile_unix_4.7kb_many_here_doc/parse_pull
                        time:   [4.6463 µs 4.6644 µs 4.6906 µs]
                        thrpt:  [975.72 MiB/s 981.19 MiB/s 985.01 MiB/s]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high severe
parse_dockerfile_unix_4.7kb_many_here_doc/parse_dom
                        time:   [5.2631 µs 5.2925 µs 5.3416 µs]
                        thrpt:  [856.79 MiB/s 864.75 MiB/s 869.58 MiB/s]
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe

dockerfile_parser_unix_4.7kb_no_here_doc/parse_dom
                        time:   [86.473 µs 86.518 µs 86.567 µs]
                        thrpt:  [52.175 MiB/s 52.204 MiB/s 52.231 MiB/s]
Found 5 outliers among 100 measurements (5.00%)
  1 (1.00%) high mild
  4 (4.00%) high severe

```


## Intel Core i7-9750H (MacBook Pro 2019, macOS 15.3)

```console
$ sysctl machdep.cpu.brand_string
machdep.cpu.brand_string: Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz

$ rustc -vV
rustc 1.86.0-nightly (854f22563 2025-01-31)
binary: rustc
commit-hash: 854f22563c8daf92709fae18ee6aed52953835cd
commit-date: 2025-01-31
host: x86_64-apple-darwin
release: 1.86.0-nightly
LLVM version: 19.1.7

$ cargo bench -p bench
<cargo log omitted>

parse_dockerfile_unix_28.3kb/parse_pull
                        time:   [80.967 µs 81.044 µs 81.119 µs]
                        thrpt:  [332.90 MiB/s 333.21 MiB/s 333.52 MiB/s]
Found 9 outliers among 100 measurements (9.00%)
  6 (6.00%) low mild
  3 (3.00%) high mild
parse_dockerfile_unix_28.3kb/parse_dom
                        time:   [92.687 µs 92.957 µs 93.213 µs]
                        thrpt:  [289.70 MiB/s 290.50 MiB/s 291.35 MiB/s]

parse_dockerfile_unix_17.6kb/parse_pull
                        time:   [57.469 µs 57.559 µs 57.647 µs]
                        thrpt:  [291.58 MiB/s 292.02 MiB/s 292.48 MiB/s]
parse_dockerfile_unix_17.6kb/parse_dom
                        time:   [67.095 µs 67.265 µs 67.444 µs]
                        thrpt:  [249.22 MiB/s 249.88 MiB/s 250.52 MiB/s]
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) low mild
  2 (2.00%) high mild

parse_dockerfile_windows_13.6kb/parse_pull
                        time:   [19.955 µs 19.965 µs 19.975 µs]
                        thrpt:  [652.76 MiB/s 653.09 MiB/s 653.41 MiB/s]
Found 3 outliers among 100 measurements (3.00%)
  1 (1.00%) high mild
  2 (2.00%) high severe
parse_dockerfile_windows_13.6kb/parse_dom
                        time:   [20.158 µs 20.170 µs 20.181 µs]
                        thrpt:  [646.08 MiB/s 646.43 MiB/s 646.81 MiB/s]
Found 8 outliers among 100 measurements (8.00%)
  1 (1.00%) low severe
  5 (5.00%) low mild
  2 (2.00%) high severe

parse_dockerfile_unix_4.7kb_no_here_doc/parse_pull
                        time:   [9.0141 µs 9.0262 µs 9.0380 µs]
                        thrpt:  [499.73 MiB/s 500.39 MiB/s 501.06 MiB/s]
parse_dockerfile_unix_4.7kb_no_here_doc/parse_dom
                        time:   [9.8018 µs 9.8172 µs 9.8331 µs]
                        thrpt:  [459.33 MiB/s 460.07 MiB/s 460.79 MiB/s]
Found 5 outliers among 100 measurements (5.00%)
  4 (4.00%) low mild
  1 (1.00%) high mild

parse_dockerfile_unix_4.7kb_many_here_doc/parse_pull
                        time:   [10.947 µs 10.975 µs 11.002 µs]
                        thrpt:  [416.00 MiB/s 417.02 MiB/s 418.08 MiB/s]
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
parse_dockerfile_unix_4.7kb_many_here_doc/parse_dom
                        time:   [12.027 µs 12.062 µs 12.097 µs]
                        thrpt:  [378.33 MiB/s 379.43 MiB/s 380.54 MiB/s]
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild

dockerfile_parser_unix_4.7kb_no_here_doc/parse_dom
                        time:   [202.00 µs 202.11 µs 202.22 µs]
                        thrpt:  [22.335 MiB/s 22.347 MiB/s 22.360 MiB/s]
Found 10 outliers among 100 measurements (10.00%)
  1 (1.00%) low severe
  5 (5.00%) low mild
  1 (1.00%) high mild
  3 (3.00%) high severe

```

*/

#![allow(dead_code)]

use std::{hint::black_box, path::PathBuf};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fs_err as fs;

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // bench
    dir
}

const UNIX_28_3KB: &str = "tests/external/moby/Dockerfile";
const UNIX_17_6KB: &str = "tests/external/buildkit/Dockerfile";
const WINDOWS_13_6KB: &str = "tests/external/moby/Dockerfile.windows";
const UNIX_4_7KB_NO_HERE_DOC: &str =
    "tests/external/moby/integration/build/testdata/Dockerfile.TestBuildPreserveOwnership";
const UNIX_4_7KB_MANY_HERE_DOC: &str =
    "tests/external/buildah/tests/conformance/testdata/Dockerfile.heredoc-quoting";

fn parse_dockerfile_benches(c: &mut Criterion) {
    fn parse_dockerfile(c: &mut Criterion, name: &str, fixture_path: &str) {
        let path = &workspace_root().join(fixture_path);
        let text = &fs::read_to_string(path).unwrap();

        let mut g = c.benchmark_group(format!("parse_dockerfile_{name}"));
        g.throughput(Throughput::Bytes(text.len() as u64));
        g.bench_function("parse_pull", move |b| {
            b.iter(|| {
                let mut p = parse_dockerfile::parse_iter(black_box(text)).unwrap();
                while let Some(i) = p.next().transpose().unwrap() {
                    black_box(i);
                }
                p
            });
        });
        g.bench_function("parse_dom", move |b| {
            b.iter(|| parse_dockerfile::parse(black_box(text)).unwrap());
        });
        g.finish();
    }

    parse_dockerfile(c, "unix_28.3kb", UNIX_28_3KB);
    parse_dockerfile(c, "unix_17.6kb", UNIX_17_6KB);
    parse_dockerfile(c, "windows_13.6kb", WINDOWS_13_6KB);
    parse_dockerfile(c, "unix_4.7kb_no_here_doc", UNIX_4_7KB_NO_HERE_DOC);
    parse_dockerfile(c, "unix_4.7kb_many_here_doc", UNIX_4_7KB_MANY_HERE_DOC);
}

// fn dockerfile_parser_benches(c: &mut Criterion) {
//     fn dockerfile_parser(c: &mut Criterion, name: &str, fixture_path: &str) {
//         let path = &workspace_root().join(fixture_path);
//         let text = &fs::read_to_string(path).unwrap();
//
//         let mut g = c.benchmark_group(format!("dockerfile_parser_{name}"));
//         g.throughput(Throughput::Bytes(text.len() as u64));
//         g.bench_function("parse_dom", move |b| {
//             b.iter(|| dockerfile_parser::Dockerfile::parse(black_box(text)).unwrap());
//         });
//         g.finish();
//     }
//
//     // dockerfile_parser(c, "unix_28.3kb", UNIX_28_3KB); // ParseError { source: Error { variant: ParsingError { positives: [EOI], negatives: [] }, location: Pos(1080), line_col: Pos((30, 51)), path: None, line: "ARG DELVE_SUPPORTED=${TARGETPLATFORM#linux/amd64} DELVE_SUPPORTED=${DELVE_SUPPORTED#linux/arm64}", continued_line: None, parse_attempts: None } }
//     // dockerfile_parser(c, "unix_17.6kb", UNIX_17_6KB); // ParseError { source: Error { variant: ParsingError { positives: [any_breakable], negatives: [] }, location: Pos(2068), line_col: Pos((56, 4)), path: None, line: "EOT␊", continued_line: None, parse_attempts: None } }
//     // dockerfile_parser(c, "windows_13.6kb", WINDOWS_13_6KB); //  ParseError { source: Error { variant: ParsingError { positives: [EOI, env_name], negatives: [] }, location: Pos(7879), line_col: Pos((173, 30)), path: None, line: "ENV GO_VERSION=${GO_VERSION} `", continued_line: None, parse_attempts: None } }
//     dockerfile_parser(c, "unix_4.7kb_no_here_doc", UNIX_4_7KB_NO_HERE_DOC);
//     // dockerfile_parser(c, "unix_4.7kb_many_here_doc", UNIX_4_7KB_MANY_HERE_DOC); //  ParseError { source: Error { variant: ParsingError { positives: [any_breakable], negatives: [] }, location: Pos(259), line_col: Pos((11, 4)), path: None, line: "EOF␊", continued_line: None, parse_attempts: None } }
// }

criterion_group!(
    benches,
    parse_dockerfile_benches,
    // Uncomment the following functions and dependencies in Cargo.toml to benchmark third-party crates.
    // dockerfile_parser_benches,
);
criterion_main!(benches);
