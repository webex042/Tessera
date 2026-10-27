//! `Sensitive<T>`: content that must never reach a log, error message or panic text.

use std::fmt;

use zeroize::Zeroize;

/// Screen text, OCR output, model output, queries, drafts — anything derived from what the user
/// saw or typed. Formatting prints `<redacted>`; use [`Sensitive::expose`] at the one place that
/// genuinely needs the value. Not `Serialize` on purpose. Zeroized on drop where `T` supports it.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Sensitive<T: Zeroize>(T);

impl<T: Zeroize> Sensitive<T> {
    pub fn new(value: T) -> Self {
        Sensitive(value)
    }

    /// Borrow the value. Grep for `expose(` to audit every use.
    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn expose_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Zeroize> Drop for Sensitive<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<T: Zeroize> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl<T: Zeroize> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Hashing does not reveal the value, so sensitive values can be map keys.
impl<T: Zeroize + std::hash::Hash> std::hash::Hash for Sensitive<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: Zeroize> From<T> for Sensitive<T> {
    fn from(v: T) -> Self {
        Sensitive(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_never_shows_the_value() {
        let s = Sensitive::new(String::from("TSRCANARY-sensitive"));
        assert_eq!(format!("{s}"), "<redacted>");
        assert_eq!(format!("{s:?}"), "<redacted>");
        assert_eq!(format!("{:?}", Some(&s)), "Some(<redacted>)");
        assert_eq!(s.expose(), "TSRCANARY-sensitive");
    }

    #[test]
    fn panic_messages_do_not_leak() {
        let s = Sensitive::new(String::from("TSRCANARY-panic"));
        let r = std::panic::catch_unwind(|| panic!("failed on {s:?}"));
        let msg = r.unwrap_err().downcast::<String>().unwrap();
        assert!(!msg.contains("TSRCANARY"), "{msg}");
    }
}
