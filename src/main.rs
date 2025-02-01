// SPDX-License-Identifier: Apache-2.0 OR MIT

#![forbid(unsafe_code)]

use std::{
    fs,
    io::{self, BufWriter, Read as _, Write as _},
    path::{Path, PathBuf},
    process::ExitCode,
};

use lexopt::Arg::{Long, Short, Value};
use parse_dockerfile::parse;

type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

macro_rules! bail {
    ($($tt:tt)*) => {
        return Err(format!($($tt)*).into())
    };
}

static USAGE: &str = "parse-dockerfile

Parse a dockerfile and output a JSON representation of all instructions in dockerfile.

USAGE:
    parse-dockerfile [OPTIONS] <PATH>

ARGS:
    <PATH>       Path to the dockerfile (use '-' for standard input)

OPTIONS:
    -h, --help                        Print help information
    -V, --version                     Print version information
";

struct Args {
    path: PathBuf,
}

impl Args {
    fn parse() -> Result<Option<Self>> {
        let mut path = None;

        let mut parser = lexopt::Parser::from_env();
        while let Some(arg) = parser.next()? {
            match arg {
                Short('h') | Long("help") => {
                    print!("{USAGE}");
                    return Ok(None);
                }
                Short('V') | Long("version") => {
                    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                Value(val) if path.is_none() => path = Some(val.into()),
                _ => return Err(arg.unexpected().into()),
            }
        }

        let Some(path) = path else { bail!("no dockerfile path specified") };

        Ok(Some(Self { path }))
    }

    fn path_for_msg(&self) -> &Path {
        if self.path.as_os_str() == "-" {
            Path::new("dockerfile (standard input)")
        } else {
            &self.path
        }
    }
}

fn main() -> ExitCode {
    if let Err(e) = try_main() {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn try_main() -> Result<()> {
    let Some(args) = Args::parse()? else { return Ok(()) };

    let text = if args.path.as_os_str() == "-" {
        let mut buf = String::with_capacity(128);
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("failed to read from standard input: {e}"))?;
        buf
    } else {
        fs::read_to_string(&args.path)
            .map_err(|e| format!("failed to read from file `{}`: {e}", args.path.display()))?
    };

    let dockerfile = match parse(&text) {
        Ok(dockerfile) => dockerfile,
        Err(e) => {
            if args.path.as_os_str() == "-" {
                bail!("{e} in {}", args.path_for_msg().display())
            }
            bail!("{e:#} at {}:{}:{}", args.path_for_msg().display(), e.line(), e.column())
        }
    };

    let mut stdout = BufWriter::new(io::stdout().lock()); // Buffered because it is written many times.
    serde_json::to_writer(&mut stdout, &dockerfile)?;
    stdout.flush()?;

    Ok(())
}
