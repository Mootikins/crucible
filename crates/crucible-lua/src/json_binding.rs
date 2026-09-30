//! `Json<T>`: a core type crossing the Lua boundary with no hand Luau type.
//!
//! A binding used to take or return a hand Luau string beside a Rust type it
//! described, and nothing held the two together — `host_api::PERMISSION_REQUEST`
//! and the table `execute_permission_hooks` built drifted this way once, and a
//! test now compares the two field lists by hand because the drift is real.
//!
//! `Json<T>` closes this for a type that has a schema. It carries `T` across
//! the boundary through `Lua::to_value`/`from_value`, the same JSON
//! conversion the rest of the host uses, and its declared Luau type is
//! `T`'s own schema, read by [`crate::signature::LuaType::of_schema`]. A
//! binding written `Json<crucible_oil::Style>` needs no declaration text at
//! all: `Ns::func` reads the type off `Json<T>`'s [`LuauValue`] impl, the way
//! it reads `bool` or `String`.
use crate::host_registry::LuauValue;
use crate::signature::LuaType;
use mlua::{FromLua, IntoLua, Lua, LuaSerdeExt, Result as LuaResult, Value};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// A core (or Oil) type, carried as JSON across the Lua boundary.
///
/// `T` must be [`utoipa::PartialSchema`] so its Luau declaration can be read
/// from its own schema, and [`Serialize`]/[`DeserializeOwned`] so it can
/// cross through `Lua::to_value`/`from_value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Json<T> {
    fn from(value: T) -> Self {
        Json(value)
    }
}

impl<T: DeserializeOwned> FromLua for Json<T> {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        lua.from_value(value).map(Json)
    }
}

impl<T: Serialize> IntoLua for Json<T> {
    fn into_lua(self, lua: &Lua) -> LuaResult<Value> {
        lua.to_value(&self.0)
    }
}

impl<T: utoipa::PartialSchema> LuauValue for Json<T> {
    fn ty() -> LuaType {
        LuaType::of_schema::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_declares_from_its_own_schema_not_a_hand_string() {
        let ty = Json::<crucible_oil::Style>::ty();
        // Every field is optional: a plugin may pass `{ fg = "red" }` with
        // every other field left out, the way `style_from_table` reads it.
        let LuaType::Record(fields) = ty else {
            panic!("Style's schema must read as a record, got {ty:?}");
        };
        let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        for expected in ["fg", "bg", "bold", "dim", "italic", "underline", "reverse"] {
            assert!(
                names.contains(&expected),
                "missing field '{expected}' in {names:?}"
            );
        }
        for field in &fields {
            assert!(
                matches!(field.ty, LuaType::Optional(_)),
                "field '{}' must be optional, got {:?}",
                field.name,
                field.ty
            );
        }
    }

    #[test]
    fn json_round_trips_through_lua() {
        let lua = Lua::new();
        let value = Json(crucible_oil::Style::new().bold());
        let lua_value = value.clone().into_lua(&lua).expect("into_lua");
        let back: Json<crucible_oil::Style> = Json::from_lua(lua_value, &lua).expect("from_lua");
        assert_eq!(back.0, value.0);
    }
}
