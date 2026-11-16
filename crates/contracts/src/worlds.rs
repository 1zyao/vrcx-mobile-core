//! World read-model contracts.
//!
//! Ported verbatim from cloud's local-data surface, which keeps the input type next
//! to the service that uses it (`WorldGetInput` in
//! cloud/crates/runtime-host-server/src/local_data.rs:94). The fork keeps pure
//! wire types in `contracts`, so the phone host, the desktop host and any future
//! transport share one definition instead of three copies.

/// Input of `app__world_get`.
///
/// Field names and defaults are cloud's: every field is optional on the wire, so a
/// caller may send only `worldId`. `force` bypasses the local cache, `full` asks for
/// the full world payload instead of the cached summary card.
#[derive(Debug, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldGetInput {
    #[serde(default)]
    pub world_id: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub full: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_get_input_matches_cloud_wire_shape() {
        // Only `worldId` is required; `force` / `full` default to false.
        let input: WorldGetInput =
            serde_json::from_str(r#"{"worldId":"wrld_c149eac3-9b97-4ed8-b019-f41171e12073"}"#)
                .unwrap();
        assert_eq!(input.world_id, "wrld_c149eac3-9b97-4ed8-b019-f41171e12073");
        assert!(!input.force);
        assert!(!input.full);

        // camelCase, and an empty input is still a valid (if useless) request.
        let input: WorldGetInput =
            serde_json::from_str(r#"{"worldId":"wrld_1","force":true,"full":true}"#).unwrap();
        assert!(input.force && input.full);

        let input: WorldGetInput = serde_json::from_str("{}").unwrap();
        assert!(input.world_id.is_empty());
    }
}
