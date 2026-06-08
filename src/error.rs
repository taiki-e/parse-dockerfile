// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::{borrow::Cow, boxed::Box, format};
use core::{fmt, marker::PhantomData, str};

use super::ParseIter;

pub(crate) type Result<T, E = Error> = core::result::Result<T, E>;
pub(crate) type InternalResult<'a, T> = core::result::Result<T, ErrorKind<'a>>;

/// An error that occurred during parsing the dockerfile.
// Boxing ErrorInner to keep error type small for performance.
// Using PhantomData to make error type !UnwindSafe & !RefUnwindSafe for forward compatibility.
pub struct Error(Box<ErrorInner>, PhantomData<Box<dyn Send + Sync>>);

impl Error {
    /// Returns the line number at which the error was detected.
    #[must_use]
    pub fn line(&self) -> usize {
        self.0.line
    }
    /// Returns the column number at which the error was detected.
    #[must_use]
    pub fn column(&self) -> usize {
        self.0.column
    }
}

#[cold]
#[inline]
pub(crate) fn other(msg: &'static str, pos: usize) -> ErrorKind<'static> {
    ErrorKind::Other { msg, pos }
}
#[cold]
#[inline]
pub(crate) fn expected(word: &'static str, pos: usize) -> ErrorKind<'static> {
    ErrorKind::Expected { word, pos }
}
#[cold]
#[inline]
pub(crate) fn expected_here_doc_end(delim: Cow<'_, [u8]>, pos: usize) -> ErrorKind<'_> {
    ErrorKind::ExpectedHereDocEnd { delim, pos }
}
#[cold]
#[inline]
pub(crate) fn expected_quote(quote: u8, found: Option<u8>, pos: usize) -> ErrorKind<'static> {
    ErrorKind::ExpectedQuote { quote, found, pos }
}
#[cold]
#[inline]
pub(crate) fn at_least_one_argument(instruction_start: usize) -> ErrorKind<'static> {
    ErrorKind::AtLeastOneArgument { instruction_start }
}
#[cold]
#[inline]
pub(crate) fn at_least_two_arguments(instruction_start: usize) -> ErrorKind<'static> {
    ErrorKind::AtLeastTwoArguments { instruction_start }
}
#[cold]
#[inline]
pub(crate) fn exactly_one_argument(instruction_start: usize) -> ErrorKind<'static> {
    ErrorKind::ExactlyOneArgument { instruction_start }
}
#[cold]
#[inline]
pub(crate) fn unknown_instruction(instruction_start: usize) -> ErrorKind<'static> {
    ErrorKind::UnknownInstruction { instruction_start }
}
#[cold]
#[inline]
pub(crate) fn invalid_escape(escape_start: usize) -> ErrorKind<'static> {
    ErrorKind::InvalidEscape { escape_start }
}
#[cold]
#[inline]
pub(crate) fn duplicate_name(name: Cow<'_, str>, second_start: usize) -> ErrorKind<'_> {
    ErrorKind::DuplicateName { name, second_start }
}
#[cold]
#[inline]
pub(crate) fn no_stage() -> ErrorKind<'static> {
    ErrorKind::NoStage
}
#[cold]
#[inline]
pub(crate) fn json(arguments_start: usize) -> ErrorKind<'static> {
    ErrorKind::Json { arguments_start }
}

#[derive(Debug)]
struct ErrorInner {
    msg: Box<str>,
    line: usize,
    column: usize,
}

#[cfg_attr(test, derive(Debug))]
pub(crate) enum ErrorKind<'a> {
    Other { msg: &'static str, pos: usize },
    Expected { word: &'static str, pos: usize },
    ExpectedHereDocEnd { delim: Cow<'a, [u8]>, pos: usize },
    ExpectedQuote { quote: u8, found: Option<u8>, pos: usize },
    AtLeastOneArgument { instruction_start: usize },
    AtLeastTwoArguments { instruction_start: usize },
    ExactlyOneArgument { instruction_start: usize },
    UnknownInstruction { instruction_start: usize },
    InvalidEscape { escape_start: usize },
    DuplicateName { name: Cow<'a, str>, second_start: usize },
    NoStage,
    Json { arguments_start: usize },
}

impl ErrorKind<'_> {
    #[cold]
    #[inline(never)]
    pub(crate) fn into_error(self, p: &ParseIter<'_>) -> Error {
        let msg = match self {
            Self::Other { msg, .. } => msg.into(),
            Self::Expected { word, .. } => format!("expected {word}").into(),
            // TODO: Truncate if the delim is large.
            Self::ExpectedHereDocEnd { ref delim, .. } => format!(
                "expected end of here-document ({}), but reached eof",
                str::from_utf8(delim).unwrap() // unwrap is okay since parsing APIs only accept &str
            )
            .into(),
            Self::ExpectedQuote { quote, found, .. } => {
                if let Some(found) = found {
                    format!(
                        "expected end of quoted string ({}), but found '{}'",
                        quote as char, found as char
                    )
                    .into()
                } else {
                    format!("expected end of quoted string ({}), but reached eof", quote as char)
                        .into()
                }
            }
            Self::AtLeastOneArgument { instruction_start: pos }
            | Self::AtLeastTwoArguments { instruction_start: pos }
            | Self::ExactlyOneArgument { instruction_start: pos }
            | Self::UnknownInstruction { instruction_start: pos } => {
                let mut s = &p.text.as_bytes()[pos..];
                let mut word = super::collect_non_whitespace(&mut s, p.text, p.escape_byte).value;
                match self {
                    Self::AtLeastOneArgument { .. } => {
                        // TODO: handle in collect_non_whitespace_unescaped
                        if word == "HEALTHCHECK" {
                            word = "HEALTHCHECK CMD".into();
                        }
                        format!("{word} instruction requires at least one argument").into()
                    }
                    Self::AtLeastTwoArguments { .. } => {
                        format!("{word} instruction requires at least two arguments").into()
                    }
                    Self::ExactlyOneArgument { .. } => {
                        format!("{word} instruction requires exactly one argument").into()
                    }
                    Self::UnknownInstruction { .. } => {
                        format!("unknown instruction '{word}'").into()
                    }
                    _ => unreachable!(),
                }
            }
            // TODO: Truncate if the delim is large.
            Self::DuplicateName { ref name, .. } => format!("duplicate stage name '{name}'").into(),
            Self::NoStage => "expected at least one FROM instruction".into(),
            Self::Json { .. } => "invalid JSON".into(),
            Self::InvalidEscape { escape_start } => {
                let mut s = &p.text.as_bytes()[escape_start..];
                super::consume_until_whitespaces_or_line_no_line_continuation(&mut s);
                let escape = &p.text[escape_start..p.text.len() - s.len()];
                format!("invalid escape '{escape}'").into()
            }
        };
        let (line, column) = match self {
            Self::Other { pos, .. }
            | Self::Expected { pos, .. }
            | Self::ExpectedHereDocEnd { pos, .. }
            | Self::ExpectedQuote { pos, .. }
            | Self::AtLeastOneArgument { instruction_start: pos }
            | Self::AtLeastTwoArguments { instruction_start: pos }
            | Self::ExactlyOneArgument { instruction_start: pos }
            | Self::UnknownInstruction { instruction_start: pos, .. }
            | Self::InvalidEscape { escape_start: pos }
            | Self::DuplicateName { second_start: pos, .. }
            | Self::Json { arguments_start: pos } => find_location_from_pos(pos, p.text.as_bytes()),
            Self::NoStage => (0, 0),
        };
        Error(Box::new(ErrorInner { msg, line, column }), PhantomData)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.line == 0 || f.alternate() {
            fmt::Display::fmt(&self.0.msg, f)
        } else {
            write!(f, "{} at line {} column {}", self.0.msg, self.0.line, self.0.column)
        }
    }
}

impl std::error::Error for Error {}

#[cold]
fn find_location_from_pos(pos: usize, text: &[u8]) -> (usize, usize) {
    let line = find_line_from_pos(pos, text);
    let column = memrchr(b'\n', text.get(..pos).unwrap_or_default()).unwrap_or(pos) + 1;
    (line, column)
}

#[cold]
fn find_line_from_pos(pos: usize, text: &[u8]) -> usize {
    bytecount(b'\n', text.get(..pos).unwrap_or_default()) + 1
}

#[inline]
const fn memrchr_naive(needle: u8, mut s: &[u8]) -> Option<usize> {
    let start = s;
    while let Some((&b, s_next)) = s.split_last() {
        if b == needle {
            return Some(start.len() - s.len());
        }
        s = s_next;
    }
    None
}
use self::memrchr_naive as memrchr;

#[inline]
const fn bytecount_naive(needle: u8, mut s: &[u8]) -> usize {
    let mut n = 0;
    while let Some((&b, s_next)) = s.split_first() {
        n += (b == needle) as usize;
        s = s_next;
    }
    n
}
use self::bytecount_naive as bytecount;
