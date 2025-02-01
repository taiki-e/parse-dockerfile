// SPDX-License-Identifier: Apache-2.0 OR MIT

/*!
Dockerfile parser, written in Rust.

# Examples

```
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

# Optional features

- **`serde`** — Implements [`serde::Serialize`] trait for parse-dockerfile types.

[`serde::Serialize`]: https://docs.rs/serde/latest/serde/trait.Serialize.html
*/

#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms, single_use_lifetimes),
        allow(dead_code, unused_variables)
    )
))]
#![forbid(unsafe_code)]
#![warn(
    // Lints that may help when writing public library.
    missing_debug_implementations,
    missing_docs,
    clippy::alloc_instead_of_core,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::impl_trait_in_params,
    // clippy::missing_inline_in_public_items,
    // clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
)]
#![allow(clippy::inline_always)]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
const _README: () = ();

#[cfg(test)]
#[path = "gen/tests/assert_impl.rs"]
mod assert_impl;

mod error;

use std::{borrow::Cow, collections::HashMap, mem, ops::Range, str};

use smallvec::SmallVec;

pub use self::error::Error;
use self::error::{ErrorKind, Result};

/// Parses dockerfile from the given `text`.
#[allow(clippy::missing_panics_doc)]
pub fn parse(text: &str) -> Result<Dockerfile<'_>> {
    let mut p = ParseIter::new(text)?;
    let mut s = p.s;

    let mut instructions = Vec::with_capacity(p.text.len() / 60);
    let mut stages = Vec::with_capacity(1);
    let mut named_stages = 0;
    let mut current_stage = None;
    while let Some((&b, s_next)) = s.split_first() {
        let instruction =
            parse_instruction(&mut p, &mut s, b, s_next).map_err(|e| e.into_error(&p))?;
        match instruction {
            Instruction::From(from) => {
                named_stages += from.as_.is_some() as usize;
                let new_stage = instructions.len();
                if let Some(prev_stage) = current_stage.replace(new_stage) {
                    stages.push(prev_stage..new_stage);
                }
                instructions.push(Instruction::From(from));
            }
            arg @ Instruction::Arg(..) => instructions.push(arg),
            instruction => {
                if current_stage.is_none() {
                    return Err(ErrorKind::Expected("FROM", instruction.instruction_span().start)
                        .into_error(&p));
                }
                instructions.push(instruction);
            }
        }
        skip_comments_and_whitespaces(&mut s, p.escape_byte);
    }
    if let Some(current_stage) = current_stage {
        stages.push(current_stage..instructions.len());
    }

    if stages.is_empty() {
        // https://github.com/moby/buildkit/blob/e83d79a51fb49aeb921d8a2348ae14a58701c98c/frontend/dockerfile/dockerfile2llb/convert.go#L263
        return Err(ErrorKind::NoStages.into_error(&p));
    }
    // TODO: https://github.com/moby/buildkit/blob/e83d79a51fb49aeb921d8a2348ae14a58701c98c/frontend/dockerfile/dockerfile2llb/convert.go#L302
    // > base name (%s) should not be blank

    let mut stages_by_name = HashMap::with_capacity(named_stages);
    for (i, stage) in stages.iter().enumerate() {
        let Instruction::From(from) = &instructions[stage.start] else { unreachable!() };
        if let Some((_as, name)) = &from.as_ {
            if let Some(first_occurrence) = stages_by_name.insert(name.value.clone(), i) {
                let Instruction::From(from) = &instructions[stages[first_occurrence].start] else {
                    unreachable!()
                };
                let first = from.as_.as_ref().unwrap().1.span.clone();
                let second = name.span.clone();
                return Err(ErrorKind::DuplicateName { first, second }.into_error(&p));
            }
        }
    }

    Ok(Dockerfile { parser_directives: p.parser_directives, instructions, stages, stages_by_name })
}

/// Returns an iterator over instructions in the given `text`.
///
/// Unlike [`parse`] function, the returned iterator doesn't error on
/// duplicate stage names.
pub fn parse_iter(text: &str) -> Result<ParseIter<'_>> {
    ParseIter::new(text)
}

/// A dockerfile.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct Dockerfile<'a> {
    /// Parser directives.
    pub parser_directives: ParserDirectives<'a>,
    /// Instructions.
    pub instructions: Vec<Instruction<'a>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    stages: Vec<Range<usize>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    stages_by_name: HashMap<Cow<'a, str>, usize>,
}
impl<'a> Dockerfile<'a> {
    /// Returns an iterator over global args.
    #[allow(clippy::missing_panics_doc)] // self.stages is not empty
    #[must_use]
    pub fn global_args<'b>(&'b self) -> impl ExactSizeIterator<Item = &'b ArgInstruction<'a>> {
        self.instructions[..self.stages.first().unwrap().start].iter().map(|arg| {
            let Instruction::Arg(arg) = arg else { unreachable!() };
            arg
        })
    }
    /// Gets a stage by name.
    #[must_use]
    pub fn stage<'b>(&'b self, name: &str) -> Option<Stage<'a, 'b>> {
        let i = *self.stages_by_name.get(name)?;
        let stage = &self.stages[i];
        let Instruction::From(from) = &self.instructions[stage.start] else { unreachable!() };
        Some(Stage { from, instructions: &self.instructions[stage.start + 1..stage.end] })
    }
    /// Returns an iterator over stages.
    #[must_use]
    pub fn stages<'b>(&'b self) -> impl ExactSizeIterator<Item = Stage<'a, 'b>> {
        self.stages.iter().map(move |stage| {
            let Instruction::From(from) = &self.instructions[stage.start] else { unreachable!() };
            Stage { from, instructions: &self.instructions[stage.start + 1..stage.end] }
        })
    }
}
/// A stage.
#[derive(Debug)]
#[non_exhaustive]
pub struct Stage<'a, 'b> {
    /// The `FROM` instruction.
    pub from: &'b FromInstruction<'a>,
    /// The remaining instructions.
    pub instructions: &'b [Instruction<'a>],
}

/// Parser directives.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#parser-directives)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct ParserDirectives<'a> {
    /// `syntax` parser directive.
    ///
    /// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#syntax)
    pub syntax: Option<ParserDirective<&'a str>>,
    /// `escape` parser directive.
    ///
    /// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#escape)
    pub escape: Option<ParserDirective<char>>,
    /// `check` parser directive.
    ///
    /// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#check)
    pub check: Option<ParserDirective<&'a str>>,
}
/// A parser directive.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct ParserDirective<T> {
    /// ```text
    /// syntax=value
    /// ^
    /// ```
    start: usize,
    /// ```text
    /// syntax=value
    ///        ^^^^^
    /// ```
    pub value: Spanned<T>,
}
impl<T> ParserDirective<T> {
    /// ```text
    /// syntax=value
    /// ^^^^^^^^^^^^
    /// ```
    #[must_use]
    pub fn span(&self) -> Span {
        self.start..self.value.span.end
    }
}

/// An instruction.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[non_exhaustive]
pub enum Instruction<'a> {
    /// `ADD` instruction.
    Add(AddInstruction<'a>),
    /// `ARG` instruction.
    Arg(ArgInstruction<'a>),
    /// `CMD` instruction.
    Cmd(CmdInstruction<'a>),
    /// `COPY` instruction.
    Copy(CopyInstruction<'a>),
    /// `ENTRYPOINT` instruction.
    Entrypoint(EntrypointInstruction<'a>),
    /// `ENV` instruction.
    Env(EnvInstruction<'a>),
    /// `EXPOSE` instruction.
    Expose(ExposeInstruction<'a>),
    /// `FROM` instruction.
    From(FromInstruction<'a>),
    /// `HEALTHCHECK` instruction.
    Healthcheck(HealthcheckInstruction<'a>),
    /// `LABEL` instruction.
    Label(LabelInstruction<'a>),
    /// `MAINTAINER` instruction (deprecated).
    Maintainer(MaintainerInstruction<'a>),
    /// `ONBUILD` instruction.
    Onbuild(OnbuildInstruction<'a>),
    /// `RUN` instruction.
    Run(RunInstruction<'a>),
    /// `SHELL` instruction.
    Shell(ShellInstruction<'a>),
    /// `STOPSIGNAL` instruction.
    Stopsignal(StopsignalInstruction<'a>),
    /// `USER` instruction.
    User(UserInstruction<'a>),
    /// `VOLUME` instruction.
    Volume(VolumeInstruction<'a>),
    /// `WORKDIR` instruction.
    Workdir(WorkdirInstruction<'a>),
}
impl Instruction<'_> {
    fn instruction_span(&self) -> Span {
        match self {
            Instruction::Add(instruction) => instruction.add.span.clone(),
            Instruction::Arg(instruction) => instruction.arg.span.clone(),
            Instruction::Cmd(instruction) => instruction.cmd.span.clone(),
            Instruction::Copy(instruction) => instruction.copy.span.clone(),
            Instruction::Entrypoint(instruction) => instruction.entrypoint.span.clone(),
            Instruction::Env(instruction) => instruction.env.span.clone(),
            Instruction::Expose(instruction) => instruction.expose.span.clone(),
            Instruction::From(instruction) => instruction.from.span.clone(),
            Instruction::Healthcheck(instruction) => instruction.healthcheck.span.clone(),
            Instruction::Label(instruction) => instruction.label.span.clone(),
            Instruction::Maintainer(instruction) => instruction.maintainer.span.clone(),
            Instruction::Onbuild(instruction) => instruction.onbuild.span.clone(),
            Instruction::Run(instruction) => instruction.run.span.clone(),
            Instruction::Shell(instruction) => instruction.shell.span.clone(),
            Instruction::Stopsignal(instruction) => instruction.stopsignal.span.clone(),
            Instruction::User(instruction) => instruction.user.span.clone(),
            Instruction::Volume(instruction) => instruction.volume.span.clone(),
            Instruction::Workdir(instruction) => instruction.workdir.span.clone(),
        }
    }
}
/// An `ADD` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#add)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct AddInstruction<'a> {
    /// ```text
    /// ADD [options] <src> ... <dest>
    /// ^^^
    /// ```
    pub add: Keyword,
    /// ```text
    /// ADD [options] <src> ... <dest>
    ///     ^^^^^^^^^
    /// ```
    pub options: SmallVec<[Flag<'a>; 1]>,
    /// ```text
    /// ADD [options] <src> ... <dest>
    ///               ^^^^^^^^^
    /// ```
    // At least 1
    pub src: SmallVec<[Source<'a>; 1]>,
    /// ```text
    /// ADD [options] <src> ... <dest>
    ///                         ^^^^^^
    /// ```
    pub dest: UnescapedString<'a>,
}
/// An `ARG` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#arg)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct ArgInstruction<'a> {
    /// ```text
    /// ARG <name>[=<default value>] [<name>[=<default value>]...]
    /// ^^^
    /// ```
    pub arg: Keyword,
    /// ```text
    /// ARG <name>[=<default value>] [<name>[=<default value>]...]
    ///     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    // TODO: SmallVec<[NameOptValue<'a>; 1]>
    pub arguments: UnescapedString<'a>,
}
/// A `CMD` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#cmd)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct CmdInstruction<'a> {
    /// ```text
    /// CMD ["executable", "param"]
    /// ^^^
    /// ```
    pub cmd: Keyword,
    /// ```text
    /// CMD ["executable", "param"]
    ///     ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: Command<'a>,
}
/// A `COPY` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#copy)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct CopyInstruction<'a> {
    /// ```text
    /// COPY [options] <src> ... <dest>
    /// ^^^^
    /// ```
    pub copy: Keyword,
    /// ```text
    /// COPY [options] <src> ... <dest>
    ///      ^^^^^^^^^
    /// ```
    pub options: SmallVec<[Flag<'a>; 1]>,
    /// ```text
    /// COPY [options] <src> ... <dest>
    ///                ^^^^^^^^^
    /// ```
    // At least 1
    pub src: SmallVec<[Source<'a>; 1]>,
    /// ```text
    /// COPY [options] <src> ... <dest>
    ///                          ^^^^^^
    /// ```
    pub dest: UnescapedString<'a>,
}
/// A enum that represents source value of [`ARG` instruction](ArgInstruction) and
/// [`COPY` instruction](CopyInstruction).
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum Source<'a> {
    /// Path or URL.
    Path(UnescapedString<'a>),
    /// Here-document.
    HereDoc(HereDoc<'a>),
}
/// An `ENTRYPOINT` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#entrypoint)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct EntrypointInstruction<'a> {
    /// ```text
    /// ENTRYPOINT ["executable", "param"]
    /// ^^^^^^^^^^
    /// ```
    pub entrypoint: Keyword,
    /// ```text
    /// ENTRYPOINT ["executable", "param"]
    ///            ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: Command<'a>,
}
/// An `ENV` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#env)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct EnvInstruction<'a> {
    /// ```text
    /// ENV <key>=<value> [<key>=<value>...]
    /// ^^^
    /// ```
    pub env: Keyword,
    /// ```text
    /// ENV <key>=<value> [<key>=<value>...]
    ///     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    // TODO: SmallVec<[NameValue<'a>; 1]>
    pub arguments: UnescapedString<'a>,
}
/// An `EXPOSE` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#expose)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct ExposeInstruction<'a> {
    /// ```text
    /// EXPOSE <port>[/<protocol>] [<port>[/<protocol>]...]
    /// ^^^^^^
    /// ```
    pub expose: Keyword,
    /// ```text
    /// EXPOSE <port>[/<protocol>] [<port>[/<protocol>]...]
    ///        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: SmallVec<[UnescapedString<'a>; 1]>,
}
/// A `FROM` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#from)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct FromInstruction<'a> {
    /// ```text
    /// FROM [--platform=<platform>] <image> [AS <name>]
    /// ^^^^
    /// ```
    pub from: Keyword,
    /// ```text
    /// FROM [--platform=<platform>] <image> [AS <name>]
    ///      ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub options: Vec<Flag<'a>>,
    /// ```text
    /// FROM [--platform=<platform>] <image> [AS <name>]
    ///                              ^^^^^^^
    /// ```
    pub image: UnescapedString<'a>,
    /// ```text
    /// FROM [--platform=<platform>] <image> [AS <name>]
    ///                                      ^^^^^^^^^^^
    /// ```
    pub as_: Option<(Keyword, UnescapedString<'a>)>,
}
/// A `HEALTHCHECK` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#healthcheck)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct HealthcheckInstruction<'a> {
    /// ```text
    /// HEALTHCHECK [options] CMD command
    /// ^^^^^^^^^^^
    /// ```
    pub healthcheck: Keyword,
    /// ```text
    /// HEALTHCHECK [options] CMD command
    ///             ^^^^^^^^^
    /// ```
    pub options: Vec<Flag<'a>>,
    /// ```text
    /// HEALTHCHECK [options] CMD command
    ///                       ^^^^^^^^^^^
    /// ```
    pub arguments: HealthcheckArguments<'a>,
}
/// Arguments of the [`HEALTHCHECK` instruction](HealthcheckInstruction).
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[non_exhaustive]
pub enum HealthcheckArguments<'a> {
    /// `HEALTHCHECK [options] CMD ...`
    #[non_exhaustive]
    Cmd {
        /// ```text
        /// HEALTHCHECK [options] CMD command
        ///                       ^^^
        /// ```
        cmd: Keyword,
        /// ```text
        /// HEALTHCHECK [options] CMD command
        ///                           ^^^^^^^
        /// ```
        arguments: Command<'a>,
    },
    /// `HEALTHCHECK [options] NONE`
    #[non_exhaustive]
    None {
        /// ```text
        /// HEALTHCHECK [options] NONE
        ///                       ^^^^
        /// ```
        none: Keyword,
    },
}
/// A `LABEL` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#label)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct LabelInstruction<'a> {
    /// ```text
    /// LABEL <key>=<value> [<key>=<value>...]
    /// ^^^^^
    /// ```
    pub label: Keyword,
    /// ```text
    /// LABEL <key>=<value> [<key>=<value>...]
    ///       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    // TODO: SmallVec<[NameValue<'a>; 1]>
    pub arguments: UnescapedString<'a>,
}
/// A `MAINTAINER` instruction (deprecated).
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#maintainer-deprecated)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct MaintainerInstruction<'a> {
    /// ```text
    /// MAINTAINER <name>
    /// ^^^^^^^^^^
    /// ```
    pub maintainer: Keyword,
    /// ```text
    /// MAINTAINER <name>
    ///            ^^^^^^
    /// ```
    pub name: UnescapedString<'a>,
}
/// A `ONBUILD` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#onbuild)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct OnbuildInstruction<'a> {
    /// ```text
    /// ONBUILD <INSTRUCTION>
    /// ^^^^^^^
    /// ```
    pub onbuild: Keyword,
    /// ```text
    /// ONBUILD <INSTRUCTION>
    ///         ^^^^^^^^^^^^^
    /// ```
    pub instruction: Box<Instruction<'a>>,
}
/// A `RUN` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#run)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct RunInstruction<'a> {
    /// ```text
    /// RUN [options] <command> ...
    /// ^^^
    /// ```
    pub run: Keyword,
    /// ```text
    /// RUN [options] <command> ...
    ///     ^^^^^^^^^
    /// ```
    pub options: SmallVec<[Flag<'a>; 1]>,
    /// ```text
    /// RUN [options] <command> ...
    ///               ^^^^^^^^^^^^^
    /// ```
    pub arguments: Command<'a>,
    /// ```text
    ///   RUN [options] <<EOF
    /// /               ^^^^^
    /// | ...
    /// | EOF
    /// |_^^^
    /// ```
    pub here_docs: Vec<HereDoc<'a>>,
}
/// A `SHELL` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#shell)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct ShellInstruction<'a> {
    /// ```text
    /// SHELL ["executable", "param"]
    /// ^^^^^
    /// ```
    pub shell: Keyword,
    /// ```text
    /// SHELL ["executable", "param"]
    ///       ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    // Usually at least 2, e.g., ["/bin/sh", "-c"]
    // Common cases are 4, e.g., ["/bin/bash", "-o", "pipefail", "-c"]
    pub arguments: SmallVec<[UnescapedString<'a>; 4]>,
}
/// A `STOPSIGNAL` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#stopsignal)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct StopsignalInstruction<'a> {
    /// ```text
    /// STOPSIGNAL signal
    /// ^^^^^^^^^^
    /// ```
    pub stopsignal: Keyword,
    /// ```text
    /// STOPSIGNAL signal
    ///            ^^^^^^
    /// ```
    pub arguments: UnescapedString<'a>,
}
/// A `USER` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#user)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct UserInstruction<'a> {
    /// ```text
    /// USER <user>[:<group>]
    /// ^^^^
    /// ```
    pub user: Keyword,
    /// ```text
    /// USER <user>[:<group>]
    ///      ^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: UnescapedString<'a>,
}
/// A `VOLUME` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#volume)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct VolumeInstruction<'a> {
    /// ```text
    /// VOLUME ["/data"]
    /// ^^^^^^
    /// ```
    pub volume: Keyword,
    /// ```text
    /// VOLUME ["/data"]
    ///        ^^^^^^^^^
    /// ```
    pub arguments: JsonOrStringArray<'a, 1>,
}
/// A `WORKDIR` instruction.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#workdir)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct WorkdirInstruction<'a> {
    /// ```text
    /// WORKDIR /path/to/workdir
    /// ^^^^^^^
    /// ```
    pub workdir: Keyword,
    /// ```text
    /// WORKDIR /path/to/workdir
    ///         ^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: UnescapedString<'a>,
}

/// A keyword.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct Keyword {
    #[allow(missing_docs)]
    pub span: Span,
}

/// An option flag.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct Flag<'a> {
    /// ```text
    /// --platform=linux/amd64
    /// ^
    /// ```
    flag_start: usize,
    /// ```text
    /// --platform=linux/amd64
    ///   ^^^^^^^^
    /// ```
    pub name: UnescapedString<'a>,
    /// ```text
    /// --platform=linux/amd64
    ///            ^^^^^^^^^^^
    /// ```
    pub value: Option<UnescapedString<'a>>,
}
impl Flag<'_> {
    /// ```text
    /// --platform=linux/amd64
    /// ^^^^^^^^^^
    /// ```
    #[must_use]
    pub fn flag_span(&self) -> Span {
        self.flag_start..self.name.span.end
    }
    /// ```text
    /// --platform=linux/amd64
    /// ^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[must_use]
    pub fn span(&self) -> Span {
        match &self.value {
            Some(v) => self.flag_start..v.span.end,
            None => self.flag_span(),
        }
    }
}

/// An unescaped string.
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct UnescapedString<'a> {
    #[allow(missing_docs)]
    pub span: Span,
    #[allow(missing_docs)]
    pub value: Cow<'a, str>,
}

/// A command.
///
/// This is used in the [`RUN`](RunInstruction), [`CMD`](CmdInstruction), and
/// [`ENTRYPOINT`](EntrypointInstruction) instructions.
///
/// [Dockerfile reference](https://docs.docker.com/reference/dockerfile/#shell-and-exec-form)
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum Command<'a> {
    /// Exec-form (JSON array)
    // At least 1
    Exec(Spanned<SmallVec<[UnescapedString<'a>; 1]>>),
    /// Shell-form (space-separated string or here-documents), escape preserved
    Shell(Spanned<&'a str>),
}

// TODO: merge two? it reduce size, but make confusing when array modified.
/// A JSON array or space-separated string.
///
/// This is used in the [`VOLUME` instruction](VolumeInstruction).
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[allow(clippy::exhaustive_enums)]
pub enum JsonOrStringArray<'a, const N: usize> {
    /// JSON array.
    Json(Spanned<SmallVec<[UnescapedString<'a>; N]>>),
    /// Space-separated string.
    String(SmallVec<[UnescapedString<'a>; N]>),
}

/// A here-document.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub struct HereDoc<'a> {
    #[allow(missing_docs)]
    pub span: Span,
    /// `false` if delimiter is quoted.
    pub expand: bool,
    #[allow(missing_docs)]
    pub value: Cow<'a, str>,
}

/// A spanned value.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde_derive::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[allow(clippy::exhaustive_structs)]
pub struct Spanned<T> {
    #[allow(missing_docs)]
    pub span: Span,
    #[allow(missing_docs)]
    pub value: T,
}

#[allow(missing_docs)]
pub type Span = Range<usize>;

// -----------------------------------------------------------------------------
// Parsing

/// An iterator over instructions.
///
/// This type is returned by [`parse_iter`] function.
#[allow(missing_debug_implementations)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct ParseIter<'a> {
    text: &'a str,
    s: &'a [u8],
    escape_byte: u8,
    has_stage: bool,
    in_onbuild: bool,
    parser_directives: ParserDirectives<'a>,
}
impl<'a> ParseIter<'a> {
    fn new(mut text: &'a str) -> Result<Self> {
        // https://github.com/moby/moby/pull/23234
        if text.as_bytes().starts_with(UTF8_BOM) {
            text = &text[UTF8_BOM.len()..];
        }
        let mut p = Self {
            text,
            s: text.as_bytes(),
            escape_byte: DEFAULT_ESCAPE_BYTE,
            has_stage: false,
            in_onbuild: false,
            parser_directives: ParserDirectives {
                // https://docs.docker.com/reference/dockerfile/#parser-directives
                syntax: None,
                escape: None,
                // https://github.com/moby/buildkit/pull/4962
                check: None,
            },
        };

        parse_parser_directives(&mut p).map_err(|e| e.into_error(&p))?;

        // https://docs.docker.com/reference/dockerfile/#format
        // > For backward compatibility, leading whitespace before comments (#) and
        // > instructions (such as RUN) are ignored, but discouraged.
        skip_comments_and_whitespaces(&mut p.s, p.escape_byte);
        Ok(p)
    }
}
impl<'a> Iterator for ParseIter<'a> {
    type Item = Result<Instruction<'a>>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let p = self;
        let mut s = p.s;
        if let Some((&b, s_next)) = s.split_first() {
            let instruction = match parse_instruction(p, &mut s, b, s_next) {
                Ok(i) => i,
                Err(e) => return Some(Err(e.into_error(p))),
            };
            match &instruction {
                Instruction::From(..) => {
                    p.has_stage = true;
                }
                Instruction::Arg(..) => {}
                instruction => {
                    if !p.has_stage {
                        return Some(Err(ErrorKind::Expected(
                            "FROM",
                            instruction.instruction_span().start,
                        )
                        .into_error(p)));
                    }
                }
            }
            skip_comments_and_whitespaces(&mut s, p.escape_byte);
            p.s = s;
            return Some(Ok(instruction));
        }
        if !p.has_stage {
            // https://github.com/moby/buildkit/blob/e83d79a51fb49aeb921d8a2348ae14a58701c98c/frontend/dockerfile/dockerfile2llb/convert.go#L263
            return Some(Err(ErrorKind::NoStages.into_error(p)));
        }
        None
    }
}

const DEFAULT_ESCAPE_BYTE: u8 = b'\\';

fn parse_parser_directives(p: &mut ParseIter<'_>) -> Result<(), ErrorKind> {
    while let Some((&b'#', s_next)) = p.s.split_first() {
        p.s = s_next;
        skip_spaces_no_escape(&mut p.s);
        let directive_start = p.text.len() - p.s.len();
        if token(&mut p.s, b"SYNTAX") {
            skip_spaces_no_escape(&mut p.s);
            if let Some((&b'=', s_next)) = p.s.split_first() {
                p.s = s_next;
                if p.parser_directives.syntax.is_some() {
                    // > Invalid due to appearing twice
                    p.parser_directives.syntax = None;
                    p.parser_directives.escape = None;
                    p.parser_directives.check = None;
                    p.escape_byte = DEFAULT_ESCAPE_BYTE;
                    skip_this_line_no_escape(&mut p.s);
                    break;
                }
                skip_spaces_no_escape(&mut p.s);
                let value_start = p.text.len() - p.s.len();
                skip_non_whitespace_no_escape(&mut p.s);
                let end = p.text.len() - p.s.len();
                let value = p.text[value_start..end].trim_ascii_end();
                p.parser_directives.syntax = Some(ParserDirective {
                    start: directive_start,
                    value: Spanned { span: value_start..value_start + value.len(), value },
                });
                skip_this_line_no_escape(&mut p.s);
                continue;
            }
        } else if token(&mut p.s, b"CHECK") {
            skip_spaces_no_escape(&mut p.s);
            if let Some((&b'=', s_next)) = p.s.split_first() {
                p.s = s_next;
                if p.parser_directives.check.is_some() {
                    // > Invalid due to appearing twice
                    p.parser_directives.syntax = None;
                    p.parser_directives.escape = None;
                    p.parser_directives.check = None;
                    p.escape_byte = DEFAULT_ESCAPE_BYTE;
                    skip_this_line_no_escape(&mut p.s);
                    break;
                }
                skip_spaces_no_escape(&mut p.s);
                let value_start = p.text.len() - p.s.len();
                skip_non_whitespace_no_escape(&mut p.s);
                let end = p.text.len() - p.s.len();
                let value = p.text[value_start..end].trim_ascii_end();
                p.parser_directives.check = Some(ParserDirective {
                    start: directive_start,
                    value: Spanned { span: value_start..value_start + value.len(), value },
                });
                skip_this_line_no_escape(&mut p.s);
                continue;
            }
        } else if token(&mut p.s, b"ESCAPE") {
            skip_spaces_no_escape(&mut p.s);
            if let Some((&b'=', s_next)) = p.s.split_first() {
                p.s = s_next;
                if p.parser_directives.escape.is_some() {
                    // > Invalid due to appearing twice
                    p.parser_directives.syntax = None;
                    p.parser_directives.escape = None;
                    p.parser_directives.check = None;
                    p.escape_byte = DEFAULT_ESCAPE_BYTE;
                    skip_this_line_no_escape(&mut p.s);
                    break;
                }
                skip_spaces_no_escape(&mut p.s);
                let value_start = p.text.len() - p.s.len();
                skip_non_whitespace_no_escape(&mut p.s);
                let end = p.text.len() - p.s.len();
                let value = p.text[value_start..end].trim_ascii_end();
                match value {
                    "`" => p.escape_byte = b'`',
                    "\\" => {}
                    _ => return Err(ErrorKind::InvalidEscape { escape_start: value_start }),
                }
                p.parser_directives.escape = Some(ParserDirective {
                    start: directive_start,
                    value: Spanned {
                        span: value_start..value_start + value.len(),
                        value: p.escape_byte as char,
                    },
                });
                skip_this_line_no_escape(&mut p.s);
                continue;
            }
        }
        skip_this_line_no_escape(&mut p.s);
        break;
    }
    Ok(())
}

#[inline]
fn parse_instruction<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    b: u8,
    s_next: &'a [u8],
) -> Result<Instruction<'a>, ErrorKind> {
    let instruction_start = p.text.len() - s.len();
    *s = s_next;
    // NB: `token_slow` must be called after all `token` calls.
    match b & TO_UPPER8 {
        b'A' => {
            if token(s, &b"ARG"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_arg(p, s, Keyword { span: instruction_span });
                }
            } else if token(s, &b"ADD"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    let add = Keyword { span: instruction_span };
                    let (options, src, dest) = parse_add_or_copy(p, s, &add)?;
                    return Ok(Instruction::Add(AddInstruction { add, options, src, dest }));
                }
            } else if token_slow(s, &b"ARG"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_arg(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"ADD"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    let add = Keyword { span: instruction_span };
                    let (options, src, dest) = parse_add_or_copy(p, s, &add)?;
                    return Ok(Instruction::Add(AddInstruction { add, options, src, dest }));
                }
            }
        }
        b'C' => {
            if token(s, &b"COPY"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    let copy = Keyword { span: instruction_span };
                    let (options, src, dest) = parse_add_or_copy(p, s, &copy)?;
                    return Ok(Instruction::Copy(CopyInstruction { copy, options, src, dest }));
                }
            } else if token(s, &b"CMD"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_cmd(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"COPY"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    let copy = Keyword { span: instruction_span };
                    let (options, src, dest) = parse_add_or_copy(p, s, &copy)?;
                    return Ok(Instruction::Copy(CopyInstruction { copy, options, src, dest }));
                }
            } else if token_slow(s, &b"CMD"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_cmd(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'E' => {
            if token(s, &b"ENV"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_env(p, s, Keyword { span: instruction_span });
                }
            } else if token(s, &b"EXPOSE"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_expose(p, s, Keyword { span: instruction_span });
                }
            } else if token(s, &b"ENTRYPOINT"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_entrypoint(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"ENV"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_env(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"EXPOSE"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_expose(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"ENTRYPOINT"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_entrypoint(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'F' => {
            if token(s, &b"FROM"[1..]) || token_slow(s, &b"FROM"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_from(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'H' => {
            if token(s, &b"HEALTHCHECK"[1..]) || token_slow(s, &b"HEALTHCHECK"[1..], p.escape_byte)
            {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_healthcheck(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'L' => {
            if token(s, &b"LABEL"[1..]) || token_slow(s, &b"LABEL"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_label(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'M' => {
            if token(s, &b"MAINTAINER"[1..]) || token_slow(s, &b"MAINTAINER"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_maintainer(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'O' => {
            if token(s, &b"ONBUILD"[1..]) || token_slow(s, &b"ONBUILD"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_onbuild(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'R' => {
            if token(s, &b"RUN"[1..]) || token_slow(s, &b"RUN"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_run(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'S' => {
            if token(s, &b"SHELL"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_shell(p, s, Keyword { span: instruction_span });
                }
            } else if token(s, &b"STOPSIGNAL"[1..]) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_stopsignal(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"SHELL"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_shell(p, s, Keyword { span: instruction_span });
                }
            } else if token_slow(s, &b"STOPSIGNAL"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_stopsignal(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'U' => {
            if token(s, &b"USER"[1..]) || token_slow(s, &b"USER"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_user(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'V' => {
            if token(s, &b"VOLUME"[1..]) || token_slow(s, &b"VOLUME"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_volume(p, s, Keyword { span: instruction_span });
                }
            }
        }
        b'W' => {
            if token(s, &b"WORKDIR"[1..]) || token_slow(s, &b"WORKDIR"[1..], p.escape_byte) {
                let instruction_span = instruction_start..p.text.len() - s.len();
                if spaces_or_line_end(s, p.escape_byte) {
                    return parse_workdir(p, s, Keyword { span: instruction_span });
                }
            }
        }
        _ => {}
    }
    Err(ErrorKind::UnknownInstruction { instruction_start })
}

#[inline]
fn parse_arg<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"ARG",
        p.escape_byte,
    ));
    let arguments = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.value.is_empty() {
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Arg(ArgInstruction { arg: instruction, arguments }))
}

#[inline]
fn parse_add_or_copy<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: &Keyword,
) -> Result<(SmallVec<[Flag<'a>; 1]>, SmallVec<[Source<'a>; 1]>, UnescapedString<'a>), ErrorKind> {
    debug_assert!(
        token_slow(&mut p.text[instruction.span.clone()].as_bytes(), b"ADD", p.escape_byte,)
            || token_slow(&mut p.text[instruction.span.clone()].as_bytes(), b"COPY", p.escape_byte,)
    );
    let options = parse_options(s, p.text, p.escape_byte);
    if is_maybe_json(s) {
        let mut tmp = *s;
        if let Ok(((src, dest), _array_span)) = parse_json_array::<(
            SmallVec<[Source<'_>; 1]>,
            Option<_>,
        )>(&mut tmp, p.text, p.escape_byte)
        {
            debug_assert!(is_line_end(tmp.first()));
            if tmp.is_empty() {
                *s = &[];
            } else {
                *s = &tmp[1..];
            }
            if src.is_empty() {
                return Err(ErrorKind::AtLeastTwoArguments {
                    instruction_start: instruction.span.start,
                });
            }
            return Ok((options, src, dest.unwrap()));
        }
    }
    let (mut src, dest) = collect_space_separated_unescaped_consume_line::<(
        SmallVec<[Source<'_>; 1]>,
        Option<_>,
    )>(s, p.text, p.escape_byte);
    if src.is_empty() {
        return Err(ErrorKind::AtLeastTwoArguments { instruction_start: instruction.span.start });
    }
    for src in &mut src {
        let Source::Path(path) = src else { unreachable!() };
        let Some(mut delim) = path.value.as_bytes().strip_prefix(b"<<") else { continue };
        if delim.is_empty() {
            continue;
        }
        let mut strip_tab = false;
        let mut quote = None;
        if let Some((&b'-', delim_next)) = delim.split_first() {
            strip_tab = true;
            delim = delim_next;
        }
        if let Some((&b, delim_next)) = delim.split_first() {
            if matches!(b, b'"' | b'\'') {
                quote = Some(b);
                delim = delim_next;
                if delim.last() != Some(&b) {
                    return Err(ErrorKind::ExpectedOwned(
                        format!(
                            "quote ({}), but found '{}'",
                            b as char,
                            *delim.last().unwrap_or(&0) as char
                        ),
                        p.text.len() - s.len(),
                    ));
                }
                delim = &delim[..delim.len() - 1];
            }
        }
        if strip_tab {
            let (here_doc, span) = collect_here_doc_strip_tab(s, p.text, p.escape_byte, delim)?;
            *src = Source::HereDoc(HereDoc { span, expand: quote.is_none(), value: here_doc });
        } else {
            let (here_doc, span) = collect_here_doc_no_strip_tab(s, p.text, p.escape_byte, delim)?;
            *src =
                Source::HereDoc(HereDoc { span, expand: quote.is_none(), value: here_doc.into() });
        }
    }
    Ok((options, src, dest.unwrap()))
}

#[allow(clippy::unnecessary_wraps)]
#[inline]
fn parse_cmd<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"CMD",
        p.escape_byte,
    ));
    if is_maybe_json(s) {
        let mut tmp = *s;
        if let Ok((arguments, array_span)) =
            parse_json_array::<SmallVec<[_; 1]>>(&mut tmp, p.text, p.escape_byte)
        {
            debug_assert!(is_line_end(tmp.first()));
            if tmp.is_empty() {
                *s = &[];
            } else {
                *s = &tmp[1..];
            }
            // "CMD []" seems to be okay?
            // https://github.com/moby/buildkit/blob/6d143f5602a61acef286f39ee75f1cb33c367d44/frontend/dockerfile/parser/testfiles/brimstone-docker-consul/Dockerfile#L3
            return Ok(Instruction::Cmd(CmdInstruction {
                cmd: instruction,
                arguments: Command::Exec(Spanned { span: array_span, value: arguments }),
            }));
        }
    }
    let arguments_start = p.text.len() - s.len();
    skip_this_line(s, p.escape_byte);
    let end = p.text.len() - s.len();
    let arguments = p.text[arguments_start..end].trim_ascii_end();
    Ok(Instruction::Cmd(CmdInstruction {
        cmd: instruction,
        arguments: Command::Shell(Spanned {
            span: arguments_start..arguments_start + arguments.len(),
            value: arguments,
        }),
    }))
}

#[inline]
fn parse_env<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"ENV",
        p.escape_byte,
    ));
    let arguments = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.value.is_empty() {
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Env(EnvInstruction { env: instruction, arguments }))
}

#[inline]
fn parse_expose<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"EXPOSE",
        p.escape_byte,
    ));
    let arguments: SmallVec<[_; 1]> =
        collect_space_separated_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.is_empty() {
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Expose(ExposeInstruction { expose: instruction, arguments }))
}

#[inline]
fn parse_entrypoint<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"ENTRYPOINT",
        p.escape_byte,
    ));
    if is_maybe_json(s) {
        let mut tmp = *s;
        if let Ok((arguments, array_span)) =
            parse_json_array::<SmallVec<[_; 1]>>(&mut tmp, p.text, p.escape_byte)
        {
            debug_assert!(is_line_end(tmp.first()));
            if tmp.is_empty() {
                *s = &[];
            } else {
                *s = &tmp[1..];
            }
            if arguments.is_empty() {
                return Err(ErrorKind::AtLeastOneArgument {
                    instruction_start: instruction.span.start,
                });
            }
            return Ok(Instruction::Entrypoint(EntrypointInstruction {
                entrypoint: instruction,
                arguments: Command::Exec(Spanned { span: array_span, value: arguments }),
            }));
        }
    }
    let arguments_start = p.text.len() - s.len();
    skip_this_line(s, p.escape_byte);
    let end = p.text.len() - s.len();
    let arguments = p.text[arguments_start..end].trim_ascii_end();
    if arguments.is_empty() {
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Entrypoint(EntrypointInstruction {
        entrypoint: instruction,
        arguments: Command::Shell(Spanned {
            span: arguments_start..arguments_start + arguments.len(),
            value: arguments,
        }),
    }))
}

#[inline]
fn parse_from<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"FROM",
        p.escape_byte,
    ));
    let options = parse_options(s, p.text, p.escape_byte);
    // TODO: https://github.com/moby/buildkit/blob/e83d79a51fb49aeb921d8a2348ae14a58701c98c/frontend/dockerfile/dockerfile2llb/convert.go#L302
    // > base name (%s) should not be blank
    let image = collect_non_whitespace_unescaped(s, p.text, p.escape_byte);
    if image.value.is_empty() {
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    let mut as_ = None;
    if skip_spaces(s, p.escape_byte) {
        let as_start = p.text.len() - s.len();
        if token(s, b"AS") || token_slow(s, b"AS", p.escape_byte) {
            let as_span = as_start..p.text.len() - s.len();
            if !skip_spaces(s, p.escape_byte) {
                return Err(ErrorKind::Expected("AS", as_start));
            }
            let name = collect_non_whitespace_unescaped(s, p.text, p.escape_byte);
            skip_spaces(s, p.escape_byte);
            if !is_line_end(s.first()) {
                return Err(ErrorKind::Expected("newline or eof", p.text.len() - s.len()));
            }
            as_ = Some((Keyword { span: as_span }, name));
        } else if !is_line_end(s.first()) {
            return Err(ErrorKind::Expected("AS", as_start));
        }
    }
    Ok(Instruction::From(FromInstruction { from: instruction, options, image, as_ }))
}

#[inline]
fn parse_healthcheck<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"HEALTHCHECK",
        p.escape_byte,
    ));
    let options = parse_options(s, p.text, p.escape_byte);
    let Some((&b, s_next)) = s.split_first() else {
        return Err(ErrorKind::Expected("CMD or NONE", p.text.len() - s.len()));
    };
    let cmd_or_none_start = p.text.len() - s.len();
    match b & TO_UPPER8 {
        b'C' => {
            *s = s_next;
            if token(s, &b"CMD"[1..]) || token_slow(s, &b"CMD"[1..], p.escape_byte) {
                let cmd_span = cmd_or_none_start..p.text.len() - s.len();
                let cmd_keyword = Keyword { span: cmd_span };
                if spaces_or_line_end(s, p.escape_byte) {
                    if is_maybe_json(s) {
                        let mut tmp = *s;
                        if let Ok((arguments, array_span)) =
                            parse_json_array::<SmallVec<[_; 1]>>(&mut tmp, p.text, p.escape_byte)
                        {
                            debug_assert!(is_line_end(tmp.first()));
                            if tmp.is_empty() {
                                *s = &[];
                            } else {
                                *s = &tmp[1..];
                            }
                            if arguments.is_empty() {
                                return Err(ErrorKind::Expected(
                                    "at least 1 arguments",
                                    array_span.start,
                                ));
                            }
                            return Ok(Instruction::Healthcheck(HealthcheckInstruction {
                                healthcheck: instruction,
                                options,
                                arguments: HealthcheckArguments::Cmd {
                                    cmd: cmd_keyword,
                                    arguments: Command::Exec(Spanned {
                                        span: array_span,
                                        value: arguments,
                                    }),
                                },
                            }));
                        }
                    }
                    let arguments_start = p.text.len() - s.len();
                    skip_this_line(s, p.escape_byte);
                    let end = p.text.len() - s.len();
                    let arguments = p.text[arguments_start..end].trim_ascii_end();
                    return Ok(Instruction::Healthcheck(HealthcheckInstruction {
                        healthcheck: instruction,
                        options,
                        arguments: HealthcheckArguments::Cmd {
                            cmd: cmd_keyword,
                            arguments: Command::Shell(Spanned {
                                span: arguments_start..arguments_start + arguments.len(),
                                value: arguments,
                            }),
                        },
                    }));
                }
            }
        }
        b'N' => {
            *s = s_next;
            if token(s, &b"NONE"[1..]) || token_slow(s, &b"NONE"[1..], p.escape_byte) {
                let none_span = cmd_or_none_start..p.text.len() - s.len();
                skip_spaces(s, p.escape_byte);
                if !is_line_end(s.first()) {
                    // TODO: error kind
                    return Err(ErrorKind::Expected(
                        "HEALTHCHECK NONE takes no arguments",
                        p.text.len() - s.len(),
                    ));
                }
                // TODO: HEALTHCHECK NONE doesn't support options
                let none_keyword = Keyword { span: none_span };
                return Ok(Instruction::Healthcheck(HealthcheckInstruction {
                    healthcheck: instruction,
                    options,
                    arguments: HealthcheckArguments::None { none: none_keyword },
                }));
            }
        }
        _ => {}
    }
    Err(ErrorKind::Expected("CMD or NONE", p.text.len() - s.len()))
}

#[inline]
fn parse_label<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"LABEL",
        p.escape_byte,
    ));
    let arguments = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.value.is_empty() {
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Label(LabelInstruction { label: instruction, arguments }))
}

#[cold]
fn parse_maintainer<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"MAINTAINER",
        p.escape_byte,
    ));
    let name = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if name.value.is_empty() {
        return Err(ErrorKind::ExactlyOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Maintainer(MaintainerInstruction { maintainer: instruction, name }))
}

#[inline]
fn parse_onbuild<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"ONBUILD",
        p.escape_byte,
    ));
    // https://docs.docker.com/reference/dockerfile/#onbuild-limitations
    if mem::replace(&mut p.in_onbuild, true) {
        // TODO: error kind
        return Err(ErrorKind::Expected("ONBUILD ONBUILD is not allowed", instruction.span.start));
    }
    let Some((&b, s_next)) = s.split_first() else {
        return Err(ErrorKind::Expected("instruction after ONBUILD", instruction.span.start));
    };
    // TODO: https://docs.docker.com/reference/dockerfile/#onbuild-limitations
    // match b & TO_UPPER8 {
    //     b'F' => {
    //         if token(s, b"FROM") || token_slow(s, b"FROM", p.escape_byte) {
    //             // TODO: error kind
    //             return Err(ErrorKind::Expected(
    //                 "ONBUILD FROM is not allowed",
    //                 instruction.span.start,
    //             ));
    //         }
    //     }
    //     b'M' => {
    //         if token(s, b"MAINTAINER")
    //             || token_slow(s, b"MAINTAINER", p.escape_byte)
    //         {
    //             // TODO: error kind
    //             return Err(ErrorKind::Expected(
    //                 "ONBUILD MAINTAINER is not allowed",
    //                 instruction.span.start,
    //             ));
    //         }
    //     }
    //     _ => {}
    // }
    let inner_instruction = parse_instruction(p, s, b, s_next)?;
    p.in_onbuild = false;
    Ok(Instruction::Onbuild(OnbuildInstruction {
        onbuild: instruction,
        instruction: Box::new(inner_instruction),
    }))
}

#[inline]
fn parse_run<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"RUN",
        p.escape_byte,
    ));
    let options = parse_options(s, p.text, p.escape_byte);
    if is_maybe_json(s) {
        let mut tmp = *s;
        if let Ok((arguments, array_span)) =
            parse_json_array::<SmallVec<[_; 1]>>(&mut tmp, p.text, p.escape_byte)
        {
            debug_assert!(is_line_end(tmp.first()));
            if tmp.is_empty() {
                *s = &[];
            } else {
                *s = &tmp[1..];
            }
            if arguments.is_empty() {
                return Err(ErrorKind::AtLeastOneArgument {
                    instruction_start: instruction.span.start,
                });
            }
            return Ok(Instruction::Run(RunInstruction {
                run: instruction,
                options,
                arguments: Command::Exec(Spanned { span: array_span, value: arguments }),
                // TODO: https://github.com/moby/buildkit/issues/2207
                here_docs: vec![],
            }));
        }
    }

    // https://docs.docker.com/reference/dockerfile/#here-documents
    let mut strip_tab = false;
    let mut quote = None;
    let mut pos = 2;
    // At least 5, <<E\nE
    if s.len() >= 5 && s.starts_with(b"<<") && {
        if s[pos] == b'-' {
            strip_tab = true;
            pos += 1;
        }
        if matches!(s[pos], b'"' | b'\'') {
            quote = Some(s[pos]);
            pos += 1;
        }
        // TODO: non-ascii_alphanumeric
        s[pos].is_ascii_alphanumeric()
    } {
        *s = &s[pos..];
        let delim_start = p.text.len() - s.len();
        // TODO: non-ascii_alphanumeric
        while let Some((&b, s_next)) = s.split_first() {
            if b.is_ascii_alphanumeric() {
                *s = s_next;
                continue;
            }
            break;
        }
        let delim = &p.text.as_bytes()[delim_start..p.text.len() - s.len()];
        if let Some(quote) = quote {
            if let Some((&b, s_next)) = s.split_first() {
                if b != quote {
                    return Err(ErrorKind::ExpectedOwned(
                        format!("quote ({}), but found '{}'", quote as char, b as char),
                        p.text.len() - s.len(),
                    ));
                }
                *s = s_next;
            } else {
                return Err(ErrorKind::ExpectedOwned(
                    format!("quote ({}), but reached eof", quote as char),
                    p.text.len() - s.len(),
                ));
            }
        }
        // TODO: skip space
        let arguments_start = p.text.len() - s.len();
        skip_this_line(s, p.escape_byte);
        let end = p.text.len() - s.len();
        let arguments = p.text[arguments_start..end].trim_ascii_end();
        let here_doc = if strip_tab {
            let (here_doc, span) = collect_here_doc_strip_tab(s, p.text, p.escape_byte, delim)?;
            HereDoc { span, expand: quote.is_none(), value: here_doc }
        } else {
            let (here_doc, span) = collect_here_doc_no_strip_tab(s, p.text, p.escape_byte, delim)?;
            HereDoc { span, expand: quote.is_none(), value: here_doc.into() }
        };
        return Ok(Instruction::Run(RunInstruction {
            run: instruction,
            options,
            arguments: Command::Shell(Spanned {
                span: arguments_start..arguments_start + arguments.len(),
                value: arguments,
            }),
            // TODO: multiple here-docs
            here_docs: vec![here_doc],
        }));
    }

    let arguments_start = p.text.len() - s.len();
    skip_this_line(s, p.escape_byte);
    let end = p.text.len() - s.len();
    let arguments = p.text[arguments_start..end].trim_ascii_end();
    Ok(Instruction::Run(RunInstruction {
        run: instruction,
        options,
        arguments: Command::Shell(Spanned {
            span: arguments_start..arguments_start + arguments.len(),
            value: arguments,
        }),
        here_docs: vec![],
    }))
}

#[inline]
fn parse_shell<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"SHELL",
        p.escape_byte,
    ));
    if !is_maybe_json(s) {
        return Err(ErrorKind::Expected("JSON array", p.text.len() - s.len()));
    }
    match parse_json_array::<SmallVec<[_; 4]>>(s, p.text, p.escape_byte) {
        Ok((arguments, _array_span)) => {
            if !s.is_empty() {
                *s = &s[1..];
            }
            if arguments.is_empty() {
                return Err(ErrorKind::AtLeastOneArgument {
                    instruction_start: instruction.span.start,
                });
            }
            Ok(Instruction::Shell(ShellInstruction { shell: instruction, arguments }))
        }
        Err(array_start) => Err(ErrorKind::Json { arguments_start: array_start }),
    }
}

#[inline]
fn parse_stopsignal<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"STOPSIGNAL",
        p.escape_byte,
    ));
    // TODO: space is disallowed?
    let arguments = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.value.is_empty() {
        return Err(ErrorKind::ExactlyOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Stopsignal(StopsignalInstruction { stopsignal: instruction, arguments }))
}

#[inline]
fn parse_user<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"USER",
        p.escape_byte,
    ));
    // TODO: space is disallowed?
    let arguments = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.value.is_empty() {
        return Err(ErrorKind::ExactlyOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::User(UserInstruction { user: instruction, arguments }))
}

#[inline]
fn parse_volume<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"VOLUME",
        p.escape_byte,
    ));
    if is_maybe_json(s) {
        let mut tmp = *s;
        if let Ok((arguments, array_span)) = parse_json_array(&mut tmp, p.text, p.escape_byte) {
            debug_assert!(is_line_end(tmp.first()));
            if tmp.is_empty() {
                *s = &[];
            } else {
                *s = &tmp[1..];
            }
            // "VOLUME []" seems to be okay?
            return Ok(Instruction::Volume(VolumeInstruction {
                volume: instruction,
                arguments: JsonOrStringArray::Json(Spanned { span: array_span, value: arguments }),
            }));
        }
    }
    let arguments: SmallVec<[_; 1]> =
        collect_space_separated_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.is_empty() {
        // TODO: "VOLUME" too?
        return Err(ErrorKind::AtLeastOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Volume(VolumeInstruction {
        volume: instruction,
        arguments: JsonOrStringArray::String(arguments),
    }))
}

#[inline]
fn parse_workdir<'a>(
    p: &mut ParseIter<'a>,
    s: &mut &'a [u8],
    instruction: Keyword,
) -> Result<Instruction<'a>, ErrorKind> {
    debug_assert!(token_slow(
        &mut p.text[instruction.span.clone()].as_bytes(),
        b"WORKDIR",
        p.escape_byte,
    ));
    // TODO: space is disallowed if not escaped/quoted?
    let arguments = collect_non_line_unescaped_consume_line(s, p.text, p.escape_byte);
    if arguments.value.is_empty() {
        return Err(ErrorKind::ExactlyOneArgument { instruction_start: instruction.span.start });
    }
    Ok(Instruction::Workdir(WorkdirInstruction { workdir: instruction, arguments }))
}

// -----------------------------------------------------------------------------
// Parsing Helpers

// [\r\n]
const LINE: u8 = 1 << 0;
// [ \t]
const SPACE: u8 = 1 << 1;
// [ \r\n\t]
const WHITESPACE: u8 = 1 << 2;
// [#]
const COMMENT: u8 = 1 << 3;
// ["]
const DOUBLE_QUOTE: u8 = 1 << 4;
// [\`]
const POSSIBLE_ESCAPE: u8 = 1 << 5;
// [=]
const EQ: u8 = 1 << 6;

static TABLE: [u8; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    loop {
        match i {
            b' ' | b'\t' => table[i as usize] = WHITESPACE | SPACE,
            b'\n' | b'\r' => table[i as usize] = WHITESPACE | LINE,
            b'#' => table[i as usize] = COMMENT,
            b'"' => table[i as usize] = DOUBLE_QUOTE,
            b'\\' | b'`' => table[i as usize] = POSSIBLE_ESCAPE,
            b'=' => table[i as usize] = EQ,
            _ => {}
        }
        if i == u8::MAX {
            break;
        }
        i += 1;
    }
    table
};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

trait Store<T>: Sized {
    fn new() -> Self;
    fn push(&mut self, val: T);
}
impl<T> Store<T> for Vec<T> {
    #[inline]
    fn new() -> Self {
        Self::new()
    }
    #[inline]
    fn push(&mut self, val: T) {
        self.push(val);
    }
}
impl<T, const N: usize> Store<T> for SmallVec<[T; N]> {
    #[inline]
    fn new() -> Self {
        Self::new()
    }
    #[inline]
    fn push(&mut self, val: T) {
        self.push(val);
    }
}
impl<'a, const N: usize> Store<UnescapedString<'a>>
    for (SmallVec<[Source<'a>; N]>, Option<UnescapedString<'a>>)
{
    #[inline]
    fn new() -> Self {
        (SmallVec::new(), None)
    }
    #[inline]
    fn push(&mut self, val: UnescapedString<'a>) {
        if let Some(val) = self.1.replace(val) {
            self.0.push(Source::Path(val));
        }
    }
}

#[inline]
fn parse_options<'a, S: Store<Flag<'a>>>(s: &mut &[u8], start: &'a str, escape_byte: u8) -> S {
    let mut options = S::new();
    'outer: loop {
        let Some((&b'-', mut s_next)) = s.split_first() else {
            break;
        };
        loop {
            let Some((&b, s_next_next)) = s_next.split_first() else {
                break 'outer;
            };
            if b == b'-' {
                s_next = s_next_next;
                break;
            }
            if skip_line_escape(&mut s_next, b, s_next_next, escape_byte) {
                skip_line_escape_followup(&mut s_next, escape_byte);
                continue;
            }
            break 'outer;
        }
        let flag_start = start.len() - s.len();
        *s = s_next;
        let name = collect_until_unescaped::<{ WHITESPACE | EQ }>(s, start, escape_byte);
        let Some((&b'=', s_next)) = s.split_first() else {
            options.push(Flag { flag_start, name, value: None });
            skip_spaces(s, escape_byte);
            continue;
        };
        *s = s_next;
        let value = collect_non_whitespace_unescaped(s, start, escape_byte);
        options.push(Flag { flag_start, name, value: Some(value) });
        skip_spaces(s, escape_byte);
    }
    options
}

fn parse_json_array<'a, S: Store<UnescapedString<'a>>>(
    s: &mut &[u8],
    start: &'a str,
    escape_byte: u8,
) -> Result<(S, Span), usize> {
    debug_assert_eq!(s.first(), Some(&b'['));
    debug_assert_ne!(s.get(1), Some(&b'['));
    let mut res = S::new();
    let array_start = start.len() - s.len();
    *s = &s[1..];
    skip_spaces(s, escape_byte);
    let (&b, s_next) = s.split_first().ok_or(array_start)?;
    match b {
        b'"' => {
            *s = s_next;
            loop {
                let full_word_start = start.len() - s.len();
                let mut word_start = full_word_start;
                let mut buf = String::new();
                loop {
                    let (&b, s_next) = s.split_first().ok_or(array_start)?;
                    if TABLE[b as usize] & (LINE | DOUBLE_QUOTE | POSSIBLE_ESCAPE) == 0 {
                        *s = s_next;
                        continue;
                    }
                    match b {
                        b'"' => break,
                        b'\n' | b'\r' => return Err(array_start),
                        _ => {}
                    }
                    let word_end = start.len() - s.len();
                    if skip_line_escape(s, b, s_next, escape_byte) {
                        skip_line_escape_followup(s, escape_byte);
                        // dockerfile escape
                        buf.push_str(&start[word_start..word_end]);
                        word_start = start.len() - s.len();
                        continue;
                    }
                    if b == b'\\' {
                        // JSON escape
                        let word_end = start.len() - s.len();
                        buf.push_str(&start[word_start..word_end]);
                        *s = s_next;
                        let (new, new_start) = match *s.first().ok_or(array_start)? {
                            b @ (b'"' | b'\\' | b'/') => (b as char, 1),
                            b'b' => ('\x08', 1),
                            b'f' => ('\x0c', 1),
                            b'n' => ('\n', 1),
                            b'r' => ('\r', 1),
                            b't' => ('\t', 1),
                            b'u' => (parse_json_hex_escape(s, array_start)?, 5),
                            _ => return Err(array_start), // invalid escape
                        };
                        buf.push(new);
                        *s = &s[new_start..];
                        word_start = start.len() - s.len();
                        continue;
                    }
                    *s = s_next;
                }
                let word_end = start.len() - s.len();
                let value = if buf.is_empty() {
                    // no escape
                    Cow::Borrowed(&start[word_start..word_end])
                } else {
                    buf.push_str(&start[word_start..word_end]);
                    Cow::Owned(buf)
                };
                res.push(UnescapedString { span: full_word_start..word_end, value });
                *s = &s[1..]; // drop "
                skip_spaces(s, escape_byte);
                let (&b, s_next) = s.split_first().ok_or(array_start)?;
                match b {
                    b',' => {
                        *s = s_next;
                        skip_spaces(s, escape_byte);
                        let (&b, s_next) = s.split_first().ok_or(array_start)?;
                        if b == b'"' {
                            *s = s_next;
                            continue;
                        }
                        return Err(array_start);
                    }
                    b']' => {
                        *s = s_next;
                        break;
                    }
                    _ => return Err(array_start),
                }
            }
        }
        b']' => *s = s_next,
        _ => return Err(array_start),
    }
    let array_end = start.len() - s.len();
    skip_spaces(s, escape_byte);
    if !is_line_end(s.first()) {
        return Err(array_start);
    }
    Ok((res, array_start..array_end))
}
// Adapted from https://github.com/serde-rs/json/blob/3f1c6de4af28b1f6c5100da323f2bffaf7c2083f/src/read.rs
#[cold]
fn parse_json_hex_escape(s: &mut &[u8], array_start: usize) -> Result<char, usize> {
    fn decode_hex_escape(s: &mut &[u8], array_start: usize) -> Result<u16, usize> {
        if s.len() < 4 {
            return Err(array_start); // EofWhileParsingString
        }

        let mut n = 0;
        for _ in 0..4 {
            let ch = decode_hex_val(s[0]);
            *s = &s[1..];
            match ch {
                None => return Err(array_start), // InvalidEscape
                Some(val) => {
                    n = (n << 4) + val;
                }
            }
        }
        Ok(n)
    }

    fn decode_hex_val(val: u8) -> Option<u16> {
        let n = HEX_DECODE_TABLE[val as usize] as u16;
        if n == u8::MAX as u16 {
            None
        } else {
            Some(n)
        }
    }

    let c = match decode_hex_escape(s, array_start)? {
        _n @ 0xDC00..=0xDFFF => return Err(array_start), // ErrorCode::LoneLeadingSurrogateInHexEscape)

        // Non-BMP characters are encoded as a sequence of two hex
        // escapes, representing UTF-16 surrogates. If deserializing a
        // utf-8 string the surrogates are required to be paired,
        // whereas deserializing a byte string accepts lone surrogates.
        n1 @ 0xD800..=0xDBFF => {
            if s.first() == Some(&b'\\') {
                *s = &s[1..];
            } else {
                return Err(array_start); // UnexpectedEndOfHexEscape
            }

            if s.first() == Some(&b'u') {
                *s = &s[1..];
            } else {
                return Err(array_start); // UnexpectedEndOfHexEscape
            }

            let n2 = decode_hex_escape(s, array_start)?;

            if n2 < 0xDC00 || n2 > 0xDFFF {
                return Err(array_start); // LoneLeadingSurrogateInHexEscape
            }

            let n = ((((n1 - 0xD800) as u32) << 10) | (n2 - 0xDC00) as u32) + 0x1_0000;

            match char::from_u32(n) {
                Some(c) => c,
                None => return Err(array_start), // InvalidUnicodeCodePoint
            }
        }

        // Every u16 outside of the surrogate ranges above is guaranteed
        // to be a legal char.
        n => char::from_u32(n as u32).unwrap(),
    };
    Ok(c)
}
#[allow(clippy::needless_raw_string_hashes)]
#[test]
fn test_parse_json_array() {
    // empty
    let t = r#"[]"#;
    let mut s = t.as_bytes();
    assert_eq!(&*parse_json_array::<Vec<_>>(&mut s, t, b'\\').unwrap().0, &[]);
    assert_eq!(s, b"");
    let t = r#"[ ]"#;
    let mut s = t.as_bytes();
    assert_eq!(&*parse_json_array::<Vec<_>>(&mut s, t, b'\\').unwrap().0, &[]);
    assert_eq!(s, b"");
    // one value
    let t = r#"["abc"]"#;
    let mut s = t.as_bytes();
    assert_eq!(&*parse_json_array::<Vec<_>>(&mut s, t, b'\\').unwrap().0, &[UnescapedString {
        span: 2..5,
        value: "abc".into()
    }]);
    assert_eq!(s, b"");
    // multi values
    let t = "[\"ab\",\"c\" ,  \"de\" ] \n";
    let mut s = t.as_bytes();
    assert_eq!(&*parse_json_array::<Vec<_>>(&mut s, t, b'\\').unwrap().0, &[
        UnescapedString { span: 2..4, value: "ab".into() },
        UnescapedString { span: 7..8, value: "c".into() },
        UnescapedString { span: 14..16, value: "de".into() },
    ]);
    assert_eq!(s, b"\n");
    // escape
    // TODO: \uXXXX
    let t = "[\"a\\\"\\\\\\/\\b\\f\\n\\r\\tbc\"]";
    let mut s = t.as_bytes();
    assert_eq!(&*parse_json_array::<Vec<_>>(&mut s, t, b'\\').unwrap().0, &[UnescapedString {
        span: 2..21,
        value: "a\"\\/\x08\x0c\n\r\tbc".into()
    }]);
    assert_eq!(s, b"");

    // fail (single quote)
    let t = r#"['abc']"#;
    let mut s = t.as_bytes();
    assert_eq!(parse_json_array::<Vec<_>>(&mut s, t, b'\\'), Err(0));
    assert_eq!(s, br#"'abc']"#);
    // fail (extra comma)
    let t = r#"["abc",]"#;
    let mut s = t.as_bytes();
    assert_eq!(parse_json_array::<Vec<_>>(&mut s, t, b'\\'), Err(0));
    assert_eq!(s, br#"]"#);
    // fail (extra char after array)
    let t = r#"["abc"] c"#;
    let mut s = t.as_bytes();
    assert_eq!(parse_json_array::<Vec<_>>(&mut s, t, b'\\'), Err(0));
    assert_eq!(s, br#"c"#);
    // fail (invalid escape)
    let t = "[\"ab\\c\"]";
    let mut s = t.as_bytes();
    assert_eq!(parse_json_array::<Vec<_>>(&mut s, t, b'\\'), Err(0));
    assert_eq!(s, b"c\"]");
    // TODO: more from https://github.com/serde-rs/json/blob/3f1c6de4af28b1f6c5100da323f2bffaf7c2083f/tests/test.rs#L1060
}

/// Skips spaces and tabs, and returns `true` if one or more spaces or tabs ware
/// consumed. (not consumes non-spaces/tabs characters.
#[inline]
fn skip_spaces_no_escape(s: &mut &[u8]) -> bool {
    let start = *s;
    while let Some((&b, s_next)) = s.split_first() {
        if TABLE[b as usize] & SPACE != 0 {
            *s = s_next;
            continue;
        }
        break;
    }
    start.len() != s.len()
}
/// Skips spaces and tabs, and returns `true` if one or more spaces or tabs ware
/// consumed. (not consumes non-space/tab characters.
#[inline]
fn skip_spaces(s: &mut &[u8], escape_byte: u8) -> bool {
    let mut has_space = false;
    while let Some((&b, s_next)) = s.split_first() {
        let t = TABLE[b as usize];
        if t & (SPACE | POSSIBLE_ESCAPE) != 0 {
            if t & SPACE != 0 {
                *s = s_next;
                has_space = true;
                continue;
            }
            if skip_line_escape(s, b, s_next, escape_byte) {
                skip_line_escape_followup(s, escape_byte);
                continue;
            }
        }
        break;
    }
    has_space
}
/// Consumes spaces and tabs, and returns `true` if one or more spaces or tabs ware
/// consumed, or reached line end. (not consumes non-space/tab characters.
#[inline]
fn spaces_or_line_end(s: &mut &[u8], escape_byte: u8) -> bool {
    let mut has_space = false;
    loop {
        let Some((&b, s_next)) = s.split_first() else { return true };
        {
            let t = TABLE[b as usize];
            if t & (WHITESPACE | POSSIBLE_ESCAPE) != 0 {
                if t & SPACE != 0 {
                    *s = s_next;
                    has_space = true;
                    continue;
                }
                if t & LINE != 0 {
                    return true;
                }
                if skip_line_escape(s, b, s_next, escape_byte) {
                    skip_line_escape_followup(s, escape_byte);
                    continue;
                }
            }
            break;
        }
    }
    has_space
}

#[inline]
fn skip_comments_and_whitespaces(s: &mut &[u8], escape_byte: u8) {
    while let Some((&b, s_next)) = s.split_first() {
        let t = TABLE[b as usize];
        if t & (WHITESPACE | COMMENT | POSSIBLE_ESCAPE) != 0 {
            if t & WHITESPACE != 0 {
                *s = s_next;
                continue;
            }
            if t & COMMENT != 0 {
                *s = s_next;
                skip_this_line_no_escape(s);
                continue;
            }
            if skip_line_escape(s, b, s_next, escape_byte) {
                skip_line_escape_followup(s, escape_byte);
                continue;
            }
        }
        break;
    }
}

#[inline]
fn is_line_end(b: Option<&u8>) -> bool {
    matches!(b, Some(b'\n' | b'\r') | None)
}
#[inline]
fn is_maybe_json(s: &[u8]) -> bool {
    // ADD/COPY: checking [[ to handle escape of [ https://docs.docker.com/reference/dockerfile/#add
    // Others: TODO: checking [[ to handle [[ -e .. ], but not enough to check [ -e .. ]
    s.first() == Some(&b'[') && s.get(1) != Some(&b'[')
}

#[inline]
fn collect_here_doc_no_strip_tab<'a>(
    s: &mut &[u8],
    start: &'a str,
    _escape_byte: u8,
    delim: &[u8],
) -> Result<(&'a str, Span), ErrorKind> {
    let here_doc_start = start.len() - s.len();
    loop {
        if s.len() < delim.len() {
            return Err(ErrorKind::ExpectedOwned(
                str::from_utf8(delim).unwrap().to_owned(),
                start.len() - s.len(),
            ));
        }
        if s.starts_with(delim) && is_line_end(s.get(delim.len())) {
            break;
        }
        skip_this_line_no_escape(s);
    }
    let end = start.len() - s.len();
    *s = &s[delim.len()..];
    if !s.is_empty() {
        *s = &s[1..];
    }
    let span = here_doc_start..end;
    Ok((&start[span.clone()], span))
}
#[inline]
fn collect_here_doc_strip_tab<'a>(
    s: &mut &[u8],
    start: &'a str,
    _escape_byte: u8,
    delim: &[u8],
) -> Result<(Cow<'a, str>, Span), ErrorKind> {
    let here_doc_start = start.len() - s.len();
    let mut current_start = here_doc_start;
    let mut res = String::new();
    loop {
        if s.len() < delim.len() {
            return Err(ErrorKind::ExpectedOwned(
                str::from_utf8(delim).unwrap().to_owned(),
                start.len() - s.len(),
            ));
        }
        if let Some((&b'\t', s_next)) = s.split_first() {
            let end = start.len() - s.len();
            res.push_str(&start[current_start..end]);
            *s = s_next;
            while let Some((&b'\t', s_next)) = s.split_first() {
                *s = s_next;
            }
            current_start = start.len() - s.len();
        }
        if s.starts_with(delim) && is_line_end(s.get(delim.len())) {
            break;
        }
        skip_this_line_no_escape(s);
    }
    let end = start.len() - s.len();
    *s = &s[delim.len()..];
    if !s.is_empty() {
        *s = &s[1..];
    }
    let span = here_doc_start..end;
    if here_doc_start == current_start {
        Ok((Cow::Borrowed(&start[span.clone()]), span))
    } else {
        res.push_str(&start[current_start..end]);
        Ok((Cow::Owned(res), span))
    }
}
// TODO: escaped/quoted space?
#[inline]
fn collect_space_separated_unescaped_consume_line<'a, S: Store<UnescapedString<'a>>>(
    s: &mut &[u8],
    start: &'a str,
    escape_byte: u8,
) -> S {
    let mut res = S::new();
    loop {
        let val = collect_non_whitespace_unescaped(s, start, escape_byte);
        if !val.value.is_empty() {
            res.push(val);
            if skip_spaces(s, escape_byte) {
                continue;
            }
        }
        debug_assert!(is_line_end(s.first()));
        if !s.is_empty() {
            *s = &s[1..];
        }
        break;
    }
    res
}
#[inline]
fn collect_non_whitespace_unescaped<'a>(
    s: &mut &[u8],
    start: &'a str,
    escape_byte: u8,
) -> UnescapedString<'a> {
    collect_until_unescaped::<WHITESPACE>(s, start, escape_byte)
}
#[inline]
fn collect_non_line_unescaped_consume_line<'a>(
    s: &mut &[u8],
    start: &'a str,
    escape_byte: u8,
) -> UnescapedString<'a> {
    let mut val = collect_until_unescaped::<LINE>(s, start, escape_byte);
    debug_assert!(is_line_end(s.first()));
    if !s.is_empty() {
        *s = &s[1..];
    }
    // trim trailing spaces
    match &mut val.value {
        Cow::Borrowed(v) => {
            while let Some(b' ' | b'\t') = v.as_bytes().last() {
                *v = &v[..v.len() - 1];
                val.span.end -= 1;
            }
        }
        Cow::Owned(v) => {
            while let Some(b' ' | b'\t') = v.as_bytes().last() {
                v.pop();
                val.span.end -= 1;
            }
        }
    }
    val
}
#[inline]
fn collect_until_unescaped<'a, const UNTIL_MASK: u8>(
    s: &mut &[u8],
    start: &'a str,
    escape_byte: u8,
) -> UnescapedString<'a> {
    let full_word_start = start.len() - s.len();
    let mut word_start = full_word_start;
    let mut buf = String::new();
    while let Some((&b, s_next)) = s.split_first() {
        let t = TABLE[b as usize];
        if t & (UNTIL_MASK | POSSIBLE_ESCAPE) != 0 {
            if t & UNTIL_MASK != 0 {
                break;
            }
            let word_end = start.len() - s.len();
            if skip_line_escape(s, b, s_next, escape_byte) {
                skip_line_escape_followup(s, escape_byte);
                buf.push_str(&start[word_start..word_end]);
                word_start = start.len() - s.len();
                continue;
            }
        }
        *s = s_next;
    }
    let word_end = start.len() - s.len();
    let value = if buf.is_empty() {
        // no escape
        Cow::Borrowed(&start[word_start..word_end])
    } else {
        buf.push_str(&start[word_start..word_end]);
        Cow::Owned(buf)
    };
    UnescapedString { span: full_word_start..word_end, value }
}

/// Skips non-whitespace (non-`[ \r\n\t]`) characters, and returns `true`
/// if one or more non-whitespace characters are present. (not consumes whitespace character).
#[inline]
fn skip_non_whitespace_no_escape(s: &mut &[u8]) -> bool {
    let start = *s;
    while let Some((&b, s_next)) = s.split_first() {
        if TABLE[b as usize] & WHITESPACE != 0 {
            break;
        }
        *s = s_next;
    }
    start.len() != s.len()
}
// #[inline]
// fn skip_non_whitespace(s: &mut &[u8], escape_byte: u8) -> bool {
//     let mut has_non_whitespace = false;
//     while let Some((&b, s_next)) = s.split_first() {
//         if TABLE[b as usize] & WHITESPACE != 0 {
//             break;
//         }
//         if is_line_escape(b, s_next, escape_byte) {
//             skip_line_escape(s, b, s_next, escape_byte);
//             continue;
//         }
//         *s = s_next;
//         has_non_whitespace = true;
//         continue;
//     }
//     has_non_whitespace
// }

#[inline]
fn skip_line_escape<'a>(s: &mut &'a [u8], b: u8, s_next: &'a [u8], escape_byte: u8) -> bool {
    if b == escape_byte {
        if let Some((&b, mut s_next)) = s_next.split_first() {
            if b == b'\n' {
                *s = s_next;
                return true;
            }
            if b == b'\r' {
                if s_next.first() == Some(&b'\n') {
                    *s = &s_next[1..];
                } else {
                    *s = s_next;
                }
                return true;
            }
            // It seems that "\\ \n" is also accepted.
            // https://github.com/moby/buildkit/blob/6d143f5602a61acef286f39ee75f1cb33c367d44/frontend/dockerfile/cmd/dockerfile-frontend/Dockerfile#L19C23-L19C24
            if TABLE[b as usize] & SPACE != 0 {
                skip_spaces_no_escape(&mut s_next);
                if let Some((&b, s_next)) = s_next.split_first() {
                    if b == b'\n' {
                        *s = s_next;
                        return true;
                    }
                    if b == b'\r' {
                        if s_next.first() == Some(&b'\n') {
                            *s = &s_next[1..];
                        } else {
                            *s = s_next;
                        }
                        return true;
                    }
                }
            }
        }
    }
    false
}
#[inline]
fn skip_line_escape_followup(s: &mut &[u8], _escape_byte: u8) {
    while let Some((&b, mut s_next)) = s.split_first() {
        let t = TABLE[b as usize];
        if t & (WHITESPACE | COMMENT) != 0 {
            if t & SPACE != 0 {
                // TODO: escape after spaces is handled in skip_spaces_no_escape
                skip_spaces_no_escape(&mut s_next);
                if let Some((&b, s_next)) = s_next.split_first() {
                    let t = TABLE[b as usize];
                    if t & (COMMENT | LINE) != 0 {
                        // comment or empty continuation line
                        *s = s_next;
                        if t & COMMENT != 0 {
                            skip_this_line_no_escape(s);
                        }
                        continue;
                    }
                }
            } else {
                // comment or empty continuation line
                *s = s_next;
                if t & COMMENT != 0 {
                    skip_this_line_no_escape(s);
                }
                continue;
            }
        }
        break;
    }
}

#[inline]
fn skip_this_line_no_escape(s: &mut &[u8]) {
    while let Some((&b, s_next)) = s.split_first() {
        *s = s_next;
        if TABLE[b as usize] & LINE != 0 {
            break;
        }
    }
}
/// Skips non-line (non-`[\r\n]`) characters. (consumes line character).
#[inline]
fn skip_this_line(s: &mut &[u8], escape_byte: u8) {
    let mut has_space_only = 0;
    while let Some((&b, s_next)) = s.split_first() {
        let t = TABLE[b as usize];
        if t & (LINE | COMMENT | POSSIBLE_ESCAPE) != 0 {
            if t & LINE != 0 {
                *s = s_next;
                break;
            }
            if has_space_only != 0 && t & COMMENT != 0 {
                *s = s_next;
                skip_this_line_no_escape(s);
                continue;
            }
            if skip_line_escape(s, b, s_next, escape_byte) {
                skip_line_escape_followup(s, escape_byte);
                has_space_only = SPACE;
                continue;
            }
        }
        has_space_only &= t;
        *s = s_next;
    }
}

#[inline(always)]
fn token(s: &mut &[u8], token: &'static [u8]) -> bool {
    let matched = starts_with_ignore_ascii_case(s, token);
    if matched {
        *s = &s[token.len()..];
        true
    } else {
        false
    }
}
#[cold]
fn token_slow(s: &mut &[u8], mut token: &'static [u8], escape_byte: u8) -> bool {
    debug_assert!(!token.is_empty() && token.iter().all(|&n| n & TO_UPPER8 == n));
    if s.len() < token.len() {
        return false;
    }
    let mut tmp = *s;
    while let Some((&b, tmp_next)) = tmp.split_first() {
        if b & TO_UPPER8 == token[0] {
            tmp = tmp_next;
            token = &token[1..];
            if token.is_empty() {
                *s = tmp;
                return true;
            }
            continue;
        }
        if skip_line_escape(&mut tmp, b, tmp_next, escape_byte) {
            skip_line_escape_followup(&mut tmp, escape_byte);
            continue;
        }
        break;
    }
    false
}

const TO_UPPER8: u8 = 0xDF;
const TO_UPPER64: u64 = 0xDFDFDFDFDFDFDFDF;

#[inline(always)] // Ensure the code getting the length of the needle is inlined.
fn starts_with_ignore_ascii_case(mut s: &[u8], mut needle: &'static [u8]) -> bool {
    debug_assert!(!needle.is_empty() && needle.iter().all(|&n| n & TO_UPPER8 == n));
    if s.len() < needle.len() {
        return false;
    }
    if needle.len() == 1 {
        return needle[0] == s[0] & TO_UPPER8;
    }
    if needle.len() >= 8 {
        loop {
            if u64::from_ne_bytes(needle[..8].try_into().unwrap())
                != u64::from_ne_bytes(s[..8].try_into().unwrap()) & TO_UPPER64
            {
                return false;
            }
            needle = &needle[8..];
            s = &s[8..];
            if needle.len() < 8 {
                if needle.is_empty() {
                    return true;
                }
                break;
            }
        }
    }
    let s = {
        let mut buf = [0; 8];
        buf[..needle.len()].copy_from_slice(&s[..needle.len()]);
        u64::from_ne_bytes(buf)
    };
    let needle = {
        let mut buf = [0; 8];
        buf[..needle.len()].copy_from_slice(needle);
        u64::from_ne_bytes(buf)
    };
    needle == s & TO_UPPER64
}
#[test]
fn test_starts_with_ignore_ascii_case() {
    assert!(starts_with_ignore_ascii_case(b"ABC", b"ABC"));
    assert!(starts_with_ignore_ascii_case(b"abc", b"ABC"));
    assert!(starts_with_ignore_ascii_case(b"AbC", b"ABC"));
    assert!(!starts_with_ignore_ascii_case(b"ABB", b"ABC"));
    assert!(starts_with_ignore_ascii_case(b"ABCDEFGH", b"ABCDEFGH"));
    assert!(starts_with_ignore_ascii_case(b"abcdefgh", b"ABCDEFGH"));
    assert!(starts_with_ignore_ascii_case(b"AbCdEfGh", b"ABCDEFGH"));
    assert!(!starts_with_ignore_ascii_case(b"ABCDEFGc", b"ABCDEFGH"));
    assert!(starts_with_ignore_ascii_case(
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    ));
    assert!(starts_with_ignore_ascii_case(
        b"abcdefghijklmnopqrstuvwxyz",
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    ));
    assert!(starts_with_ignore_ascii_case(
        b"aBcDeFgHiJkLmNoPqRsTuVwXyZ",
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    ));
    assert!(!starts_with_ignore_ascii_case(
        b"aBcDeFgHiJkLmNoPqRsTuVwXyc",
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    ));
}

// Lookup table for ascii to hex decoding.
#[rustfmt::skip]
static HEX_DECODE_TABLE: [u8; 256] = {
    const __: u8 = u8::MAX;
    [
        //  _1  _2  _3  _4  _5  _6  _7  _8  _9  _A  _B  _C  _D  _E  _F
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 0_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 1_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 2_
         0,  1,  2,  3,  4,  5,  6,  7,  8,  9, __, __, __, __, __, __, // 3_
        __, 10, 11, 12, 13, 14, 15, __, __, __, __, __, __, __, __, __, // 4_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 5_
        __, 10, 11, 12, 13, 14, 15, __, __, __, __, __, __, __, __, __, // 6_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 7_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 8_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 9_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // A_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // B_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // C_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // D_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // E_
        __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // F_
    ]
};
