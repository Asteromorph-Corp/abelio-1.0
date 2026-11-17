//! JSON-schema inspection tests for every structured-output type used by a non-base agent.
//!
//! Each test generates `schema_for!(T)`, asserts the schema has non-empty properties, and prints
//! the full schema for inspection. String-output agents (enhance, goal_simplifier,
//! element_selector, proposal_maker, reviewer) don't emit a JSON schema, so they are intentionally
//! omitted.
//!
//! Run with `cargo test --test agent_schemas -- --nocapture` to see the rendered schemas.

use abelio_ideation::testing::*;

fn dump(label: &str, schema: &schemars::Schema) {
    let pretty = serde_json::to_string_pretty(schema).unwrap_or_else(|_| format!("{schema:?}"));
    println!("\n===== schema: {label} =====\n{pretty}\n=====");
}

fn assert_has_properties(label: &str, schema: &schemars::Schema) {
    let json = serde_json::to_value(schema).expect("schema is not serializable");
    let has_props = json
        .as_object()
        .and_then(|o| o.get("properties"))
        .and_then(|v| v.as_object())
        .is_some_and(|p| !p.is_empty());
    assert!(has_props, "schema for {label} has no properties: {json}");
}

macro_rules! schema_test {
    ($name:ident, $ty:ty) => {
        #[test]
        fn $name() {
            let schema = schemars::schema_for!($ty);
            assert_has_properties(stringify!($ty), &schema);
            dump(stringify!($ty), &schema);
        }
    };
}

schema_test!(formulations, Formulations);
schema_test!(found_elements, FoundElements);
schema_test!(distilled_elements, DistilledElements);
schema_test!(alternative_entities, AlternativeEntities);
schema_test!(requirement, Requirement);
schema_test!(filtered_elements, FilteredElements);
schema_test!(refinement, Refinement);
schema_test!(judge_verdict, JudgeVerdict);
