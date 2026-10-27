//! Understanding pipeline (plan §8): what happens to an item after a window session is stored.
//!
//! - [`dates`] — deterministic date resolver (§8.4).
//! - [`audit`] — re-validates stored facts and evidence ("0 schema-invalid rows").
//! - [`normalize`] / [`evidence`] — text folding with source offsets; evidence must be in the item.
//! - [`schema`] — strict parsing of the model's extraction output (§16.1).
//! - [`guard`] — instruction classification: document content is data, not authority (§6.4).
//! - [`people`] — conservative people resolution and the suppression list (§8.5).
//! - [`extract`] — the extraction job: model call, validation, evidence, second redaction,
//!   de-duplication and one atomic write (§8.2–8.3).
//! - [`worker`] — durable `jobs` rows → scheduler jobs, and the runners that execute them.

pub mod askq;
pub mod audit;
pub mod brief;
pub mod dates;
pub mod evidence;
pub mod extract;
pub mod guard;
pub mod loops;
pub mod normalize;
pub mod people;
pub mod rollup;
pub mod schema;
pub mod worker;
