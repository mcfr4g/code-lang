use std::cell::RefCell;
use std::io::{self, Cursor, Write};

const OUTPUT_LIMIT: usize = 64 * 1024;

struct Output(Vec<u8>);

impl Write for Output {
    fn write(&mut self, text: &[u8]) -> io::Result<usize> {
        if text.len() > OUTPUT_LIMIT - self.0.len() {
            return Err(io::Error::other("playground output limit reached (64 KiB)"));
        }
        self.0.extend_from_slice(text);
        Ok(text.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Session {
    source: Vec<u8>,
    answers: Vec<u8>,
    output: Output,
    error: String,
}

thread_local! {
    static SESSION: RefCell<Session> = const { RefCell::new(Session {
        source: Vec::new(),
        answers: Vec::new(),
        output: Output(Vec::new()),
        error: String::new(),
    }) };
}

// JavaScript fills this buffer before invoking code_run. It remains allocated
// until the next call to code_input or until the worker is closed.
#[no_mangle]
pub extern "C" fn code_input(length: usize) -> *mut u8 {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.source.resize(length, 0);
        session.source.as_mut_ptr()
    })
}

#[no_mangle]
pub extern "C" fn code_answers(length: usize) -> *mut u8 {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        session.answers.resize(length, 0);
        session.answers.as_mut_ptr()
    })
}

#[no_mangle]
pub extern "C" fn code_run() -> i32 {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        let Session {
            source,
            answers,
            output,
            error,
        } = &mut *session;
        output.0.clear();
        error.clear();
        let Ok(source) = std::str::from_utf8(source) else {
            *error = "Expected UTF-8 Code source".to_owned();
            return 1;
        };
        match crate::run_with_input(source, &mut Cursor::new(answers.as_slice()), output) {
            Ok(()) => 0,
            Err(problem) if problem.is_awaiting_input() => 2,
            Err(problem) => {
                *error = problem.to_string();
                1
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn code_output_ptr() -> *const u8 {
    SESSION.with(|session| session.borrow().output.0.as_ptr())
}

#[no_mangle]
pub extern "C" fn code_output_len() -> usize {
    SESSION.with(|session| session.borrow().output.0.len())
}

#[no_mangle]
pub extern "C" fn code_error_ptr() -> *const u8 {
    SESSION.with(|session| session.borrow().error.as_ptr())
}

#[no_mangle]
pub extern "C" fn code_error_len() -> usize {
    SESSION.with(|session| session.borrow().error.len())
}
