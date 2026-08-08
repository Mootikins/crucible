//! Deep merge for TOML values.

/// Merge two TOML values, with source overriding target for conflicts.
///
/// Tables merge key by key, arrays concatenate, and any other type is
/// replaced outright. `{file:…}` and `{dir:…}` references both fold their
/// resolved content in through this.
#[cfg(feature = "toml")]
pub(super) fn merge_toml_values(target: &mut toml::Value, source: &toml::Value) {
    match (target, source) {
        (toml::Value::Table(target_table), toml::Value::Table(source_table)) => {
            for (key, source_value) in source_table {
                if let Some(target_value) = target_table.get_mut(key) {
                    merge_toml_values(target_value, source_value);
                } else {
                    target_table.insert(key.clone(), source_value.clone());
                }
            }
        }
        (toml::Value::Array(target_array), toml::Value::Array(source_array)) => {
            target_array.extend(source_array.iter().cloned());
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}
