mod error;
mod number;
mod runtime;
mod syntax;
#[cfg(target_arch = "wasm32")]
mod web;

use std::borrow::Cow;
use std::io::{self, BufRead, Write};

pub use error::CodeError;

pub fn run(source: &str, output: &mut impl Write) -> Result<(), CodeError> {
    run_with_input(source, &mut io::empty(), output)
}

pub fn run_with_input(
    source: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), CodeError> {
    let source = if source.contains('\r') {
        Cow::Owned(source.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(source)
    };
    runtime::run(&source, input, output)
}
