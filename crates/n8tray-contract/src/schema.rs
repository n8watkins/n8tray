//! The contract's JSON Schemas, embedded from `spec/schema/` and compiled once.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

/// `spec/schema/manifest.schema.json`.
pub const MANIFEST: &str = include_str!("../../../spec/schema/manifest.schema.json");
/// `spec/schema/status.schema.json`.
pub const STATUS: &str = include_str!("../../../spec/schema/status.schema.json");
/// `spec/schema/lease.schema.json`.
pub const LEASE: &str = include_str!("../../../spec/schema/lease.schema.json");
/// `spec/schema/action-result.schema.json`.
pub const ACTION_RESULT: &str = include_str!("../../../spec/schema/action-result.schema.json");
/// `spec/schema/modular-v1.schema.json`.
pub const MODULAR_V1: &str = include_str!("../../../spec/schema/modular-v1.schema.json");

fn compile(text: &str) -> Validator {
    let schema: Value = serde_json::from_str(text).expect("an embedded schema is valid JSON");
    jsonschema::draft202012::new(&schema).expect("an embedded schema compiles")
}

pub(crate) static MANIFEST_VALIDATOR: LazyLock<Validator> = LazyLock::new(|| compile(MANIFEST));
pub(crate) static STATUS_VALIDATOR: LazyLock<Validator> = LazyLock::new(|| compile(STATUS));
pub(crate) static LEASE_VALIDATOR: LazyLock<Validator> = LazyLock::new(|| compile(LEASE));
pub(crate) static ACTION_RESULT_VALIDATOR: LazyLock<Validator> =
    LazyLock::new(|| compile(ACTION_RESULT));
pub(crate) static MODULAR_V1_VALIDATOR: LazyLock<Validator> = LazyLock::new(|| compile(MODULAR_V1));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_compiles_against_draft_2020_12() {
        for text in [MANIFEST, STATUS, LEASE, ACTION_RESULT, MODULAR_V1] {
            let schema: Value = serde_json::from_str(text).unwrap();
            assert_eq!(
                schema["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
            jsonschema::meta::validate(&schema)
                .expect("the schema is a valid draft 2020-12 schema");
            compile(text);
        }
    }
}
