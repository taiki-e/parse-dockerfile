// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::borrow::Cow;
use core::{fmt, marker::PhantomData, ops::Range, str};

use super::ParseIter;

pub(crate) type Result<T, E = Error> = core::result::Result<T, E>;

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

#[derive(Debug)]
struct ErrorInner {
    msg: Cow<'static, str>,
    line: usize,
    column: usize,
}

#[cfg_attr(test, derive(Debug))]
pub(crate) enum ErrorKind {
    Expected(&'static str, /* pos */ usize),
    ExpectedOwned(String, /* pos */ usize),
    AtLeastOneArgument { instruction_start: usize },
    AtLeastTwoArguments { instruction_start: usize },
    ExactlyOneArgument { instruction_start: usize },
    UnknownInstruction { instruction_start: usize },
    InvalidEscape { escape_start: usize },
    DuplicateName { first: Range<usize>, second: Range<usize> },
    NoStages,
    Json { arguments_start: usize },
}

impl ErrorKind {
    #[cold]
    #[inline(never)]
    pub(crate) fn into_error(self, p: &ParseIter<'_>) -> Error {
        let msg = match self {
            Self::Expected(msg, ..) => format!("expected {msg}").into(),
            Self::ExpectedOwned(ref msg, ..) => format!("expected {msg}").into(),
            Self::AtLeastOneArgument { instruction_start: pos }
            | Self::AtLeastTwoArguments { instruction_start: pos }
            | Self::ExactlyOneArgument { instruction_start: pos }
            | Self::UnknownInstruction { instruction_start: pos }
            | Self::DuplicateName { first: Range { start: pos, .. }, .. } => {
                let mut s = &p.text.as_bytes()[pos..];
                let word =
                    super::collect_non_whitespace_unescaped(&mut s, p.text, p.escape_byte).value;
                match self {
                    Self::AtLeastOneArgument { .. } => {
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
                    Self::DuplicateName { .. } => format!("duplicate name '{word}'").into(),
                    _ => unreachable!(),
                }
            }
            Self::NoStages => "expected at least one FROM instruction".into(),
            Self::Json { .. } => "invalid JSON".into(),
            Self::InvalidEscape { escape_start } => {
                let mut s = &p.text.as_bytes()[escape_start..];
                super::skip_non_whitespace_no_escape(&mut s);
                let escape = &p.text[escape_start..p.text.len() - s.len()];
                format!("invalid escape '{escape}'").into()
            }
        };
        let (line, column) = match self {
            Self::Expected(_, pos)
            | Self::ExpectedOwned(_, pos)
            | Self::AtLeastOneArgument { instruction_start: pos }
            | Self::AtLeastTwoArguments { instruction_start: pos }
            | Self::ExactlyOneArgument { instruction_start: pos }
            | Self::UnknownInstruction { instruction_start: pos, .. }
            | Self::InvalidEscape { escape_start: pos }
            | Self::DuplicateName { second: Range { start: pos, .. }, .. }
            | Self::Json { arguments_start: pos } => find_location_from_pos(pos, p.text.as_bytes()),
            Self::NoStages => (0, 0),
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
