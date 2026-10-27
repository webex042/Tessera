//! Password-screen check for OCR text (plan §6.2). Platform-neutral so it is testable anywhere.

/// Plan §6.2: when rung 1 could not inspect a window, OCR text that shows a password-like label
/// next to a run of mask characters means a credential prompt; such windows are not stored.
pub fn looks_like_password_screen(ocr_text: &str) -> bool {
    let lower = ocr_text.to_lowercase();
    let label = [
        "password",
        "passcode",
        "पासवर्ड",
        " pin",
        "pin:",
        "passwort",
        "mot de passe",
        "contraseña",
    ]
    .iter()
    .any(|l| lower.contains(l));
    let masked = ocr_text.lines().any(|l| {
        let mut run = 0;
        let mut best = 0;
        for c in l.chars() {
            if matches!(c, '•' | '●' | '*' | '·' | '∙' | '⬤') {
                run += 1;
                best = best.max(run);
            } else if !c.is_whitespace() {
                run = 0;
            }
        }
        best >= 4
    });
    label && masked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_screen_heuristic() {
        assert!(looks_like_password_screen(
            "Sign in\nEmail asha@example.com\nPassword\n••••••••\nNext"
        ));
        assert!(looks_like_password_screen("PIN: * * * *"));
        assert!(!looks_like_password_screen("Password policy: at least 12 characters"));
        assert!(
            !looks_like_password_screen("Rating ★★★★ •••• great"),
            "dots without a label"
        );
    }
}
