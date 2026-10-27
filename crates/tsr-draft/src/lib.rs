//! Draft assistant (plan §12): build the request, stream the draft, check it by code.
//!
//! - [`factcheck`]: every checkable claim in a draft (dates, times, weekdays, amounts, numbers,
//!   e-mail addresses, URLs, file names, capitalised names) must appear in the thread, the memory
//!   snippets or the user's instruction; otherwise it is flagged "not found in context". New
//!   commitments the draft makes for the user are flagged too, so the user notices them.
//! - [`draft`]: the request (prompt `draft/v1`), strict output parsing, and the streaming view of
//!   the draft text while the JSON is still arriving.
//!
//! Nothing here inserts or sends anything: insertion is the app's job, after the user chooses
//! Insert, and only into the field that was focused when the draft was requested.

pub mod draft;
pub mod factcheck;
