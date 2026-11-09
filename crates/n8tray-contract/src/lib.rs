//! The N8 Tray contract, version 2: the manifest an app installs, the status
//! it reports, the result of an action and the lease the host writes.
//!
//! The JSON Schemas in `spec/schema/` are normative for the shape of every
//! document; [`validate`] checks a document against its schema and then the
//! rules a schema cannot express (spec section 11). The types in [`manifest`],
//! [`status`], [`lease`] and [`action`] read valid documents, ignoring unknown
//! fields as a host does at run time (spec section 10).

pub mod action;
pub mod lease;
pub mod manifest;
pub mod paths;
pub mod schema;
pub mod status;
pub mod validate;

pub use validate::{Issue, Kind, Report, validate, validate_as};

/// The contract major version this crate implements.
pub const CONTRACT: u32 = 2;

/// Removes a UTF-8 byte order mark, which Windows PowerShell writes and every
/// reader must accept (spec 5.2 and 8.2).
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Parses a JSON document, accepting a UTF-8 byte order mark.
pub fn parse_json(text: &str) -> serde_json::Result<serde_json::Value> {
    serde_json::from_str(strip_bom(text))
}

#[cfg(test)]
mod tests {
    #[test]
    fn implements_contract_2() {
        assert_eq!(super::CONTRACT, 2);
    }

    #[test]
    fn parse_json_accepts_a_bom() {
        let value = super::parse_json("\u{feff}{\"a\": 1}").unwrap();
        assert_eq!(value["a"], 1);
    }
}
