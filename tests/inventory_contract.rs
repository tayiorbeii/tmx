use std::collections::HashSet;

use tmx::switcher::contract::{InventoryEnvelope, RouteResponse, INVENTORY_SCHEMA, SCHEMA_MAJOR};

#[test]
fn canonical_complete_fixture_round_trips_without_identity_or_order_drift() {
    let fixture = include_str!("fixtures/inventory/v1/complete.json");
    let parsed: InventoryEnvelope = serde_json::from_str(fixture).unwrap();
    assert_eq!(parsed.schema.name, INVENTORY_SCHEMA);
    assert_eq!(parsed.schema.major, SCHEMA_MAJOR);
    assert!(parsed.complete);
    let canonical = serde_json::to_string(&parsed).unwrap();
    let reparsed: InventoryEnvelope = serde_json::from_str(&canonical).unwrap();
    assert_eq!(parsed, reparsed);
}

#[test]
fn additive_minor_fields_are_tolerated_but_incompatible_major_is_visible() {
    let additive: InventoryEnvelope =
        serde_json::from_str(include_str!("fixtures/inventory/v1/additive-minor.json")).unwrap();
    assert_eq!(additive.schema.major, 1);
    assert_eq!(additive.schema.minor, 9);

    let incompatible: InventoryEnvelope = serde_json::from_str(include_str!(
        "fixtures/inventory/v1/incompatible-major.json"
    ))
    .unwrap();
    assert_eq!(incompatible.schema.major, 2);
    assert_ne!(incompatible.schema.major, SCHEMA_MAJOR);
}

#[test]
fn endpoint_qualified_identity_keeps_colliding_runtime_ids_distinct() {
    let parsed: InventoryEnvelope =
        serde_json::from_str(include_str!("fixtures/inventory/v1/id-collision.json")).unwrap();
    let keys = parsed
        .endpoints
        .iter()
        .flat_map(|endpoint| {
            endpoint.sessions.iter().map(|session| {
                (
                    endpoint.host_domain.clone(),
                    endpoint.endpoint_id.clone(),
                    session.generation.clone(),
                    session.session_id.clone(),
                )
            })
        })
        .collect::<HashSet<_>>();
    assert_eq!(keys.len(), 2);
    assert_eq!(
        parsed.endpoints[0].sessions[0].session_id,
        parsed.endpoints[1].sessions[0].session_id
    );
}

#[test]
fn partial_fixture_preserves_healthy_endpoint_and_typed_failure() {
    let parsed: InventoryEnvelope =
        serde_json::from_str(include_str!("fixtures/inventory/v1/partial-failure.json")).unwrap();
    assert!(!parsed.complete);
    assert_eq!(parsed.endpoints.len(), 2);
    assert!(parsed.endpoints[0].diagnostics.is_empty());
    assert_eq!(parsed.endpoints[1].diagnostics[0].code, "timeout");
}

#[test]
fn missing_required_contract_fields_fail_deserialization() {
    let missing_schema = r#"{"request_id":"x"}"#;
    assert!(serde_json::from_str::<InventoryEnvelope>(missing_schema).is_err());
    let wrong_type = include_str!("fixtures/inventory/v1/complete.json")
        .replace("\"complete\":true", "\"complete\":\"yes\"");
    assert!(serde_json::from_str::<InventoryEnvelope>(&wrong_type).is_err());
}

#[test]
fn every_route_outcome_has_a_stable_machine_spelling() {
    let responses: Vec<RouteResponse> =
        serde_json::from_str(include_str!("fixtures/route/v1/outcomes.json")).unwrap();
    let actual = responses
        .iter()
        .map(|response| serde_json::to_value(response).unwrap()["outcome"].clone())
        .collect::<Vec<_>>();
    let expected = [
        "success",
        "success_new_attachment",
        "stale_target",
        "stale_client",
        "unavailable_endpoint",
        "untrusted_endpoint",
        "incompatible_schema",
        "timeout",
        "command_failure",
        "partial_success",
    ];
    assert_eq!(actual.len(), expected.len());
    for (value, spelling) in actual.iter().zip(expected) {
        assert_eq!(value, spelling);
    }
}
