//! Optional backtraces captured at error conversion and diagnostic boundaries.

#[cfg(feature = "error-backtrace")]
use std::{backtrace::Backtrace, panic::Location, sync::Arc};
use std::{error::Error, fmt};

/// Diagnostic metadata, excluded from error equality and empty without capture enabled.
#[derive(Clone, Default)]
pub struct ErrorTrace {
    #[cfg(feature = "error-backtrace")]
    capture: Option<Arc<Capture>>,
}

#[cfg(feature = "error-backtrace")]
struct Capture {
    boundary: &'static str,
    location: &'static Location<'static>,
    backtrace: Backtrace,
}

impl ErrorTrace {
    /// Captures the full current stack, naming the boundary rather than claiming a leaf origin.
    #[track_caller]
    pub fn capture(boundary: &'static str) -> Self {
        #[cfg(not(feature = "error-backtrace"))]
        let _ = boundary;
        Self {
            #[cfg(feature = "error-backtrace")]
            capture: Some(Arc::new(Capture {
                boundary,
                location: Location::caller(),
                backtrace: Backtrace::force_capture(),
            })),
        }
    }

    /// Formats every available frame, together with the capture boundary and source location.
    pub fn full_backtrace(&self) -> Option<String> {
        #[cfg(feature = "error-backtrace")]
        {
            self.capture.as_ref().map(|capture| {
                format!(
                    "Backtrace captured at {}: {}\n{:#}",
                    capture.boundary, capture.location, capture.backtrace
                )
            })
        }
        #[cfg(not(feature = "error-backtrace"))]
        {
            None
        }
    }
}

impl fmt::Debug for ErrorTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(feature = "error-backtrace")]
        if let Some(capture) = &self.capture {
            return write!(f, "{} at {}", capture.boundary, capture.location);
        }
        f.write_str("backtrace unavailable")
    }
}

impl PartialEq for ErrorTrace {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ErrorTrace {}

/// An existing error with a backtrace attached when it crosses an error conversion.
#[derive(Debug)]
pub struct TracedError<E> {
    error: E,
    trace: ErrorTrace,
}

impl<E> TracedError<E> {
    /// Retains the original error and captures the conversion's calling stack.
    #[track_caller]
    pub fn new(error: E) -> Self {
        Self {
            error,
            trace: ErrorTrace::capture("error conversion"),
        }
    }

    /// Borrows the original typed error for recovery decisions.
    pub fn error(&self) -> &E {
        &self.error
    }

    /// Borrows the trace captured during conversion.
    pub fn trace(&self) -> &ErrorTrace {
        &self.trace
    }
}

impl<E: fmt::Display> fmt::Display for TracedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl<E: Error + 'static> Error for TracedError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

/// Formats an error's complete source chain and the supplied boundary backtrace.
pub fn format_error(error: &(dyn Error + 'static), trace: &ErrorTrace) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        text.push_str(&format!("\nCaused by: {error}"));
        source = error.source();
    }
    text.push('\n');
    text.push_str(
        &trace
            .full_backtrace()
            .unwrap_or_else(|| "Backtrace unavailable (enable error-backtrace).".to_owned()),
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_metadata_does_not_change_equality() {
        let first = ErrorTrace::capture("first boundary");
        let second = ErrorTrace::capture("second boundary");
        assert_eq!(first, second);
        assert_eq!(first.full_backtrace(), first.clone().full_backtrace());
    }

    #[test]
    #[cfg(not(feature = "error-backtrace"))]
    fn disabled_capture_has_no_storage_or_backtrace() {
        assert_eq!(std::mem::size_of::<ErrorTrace>(), 0);
        assert!(ErrorTrace::capture("fixture").full_backtrace().is_none());
    }
}
