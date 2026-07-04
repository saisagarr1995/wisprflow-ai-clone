use serde_json::Value;

use crate::schema::CONFIG_VERSION;

/// Migrate a raw config JSON value from `from_version` up to `CONFIG_VERSION`,
/// one version step at a time. Each step receives the previous step's output,
/// so migrations compose and only ever need to know about adjacent versions.
pub fn migrate(from_version: u32, mut value: Value) -> Result<Value, String> {
    let mut version = from_version;
    while version < CONFIG_VERSION {
        value = match version {
            0 => migrate_v0_to_v1(value)?,
            v => return Err(format!("no migration path from config version {v}")),
        };
        version += 1;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("version".into(), Value::from(version));
        }
    }
    Ok(value)
}

/// v0 = pre-versioning (no `version` field). Structurally identical to v1:
/// unknown fields are dropped and missing ones take defaults on deserialize,
/// so there is nothing to rewrite.
fn migrate_v0_to_v1(value: Value) -> Result<Value, String> {
    Ok(value)
}
