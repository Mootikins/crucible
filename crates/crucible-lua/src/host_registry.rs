//! Registering a host function and declaring its type, in one statement.
//!
//! The declarations used to live in a table beside the registrations. Six of
//! the first twenty were wrong — `cru.timer.sleep` was declared in
//! milliseconds while the closure takes seconds, `cru.fs.mkdir` was declared
//! `-> boolean` while it returns nothing — and none of them were type errors,
//! so no tool could report them. They were wrong because nothing held the
//! declaration to the code it described.
//!
//! [`Ns`] does. `ns.func(name, "(path: string) -> ()", closure)` registers the
//! closure AND declares its type, and two things follow:
//!
//! - **At compile time**, the bound. `A: LuauArgs` and `R: LuauValue` mean a
//!   closure whose argument or return type the host cannot name does not
//!   compile. No function reaches a module table undescribed.
//! - **At registration**, the comparison. The declaration is parsed and
//!   checked against the Rust types: arity, primitives and optionality must
//!   agree. A mismatch fails the VM's construction, so every test that builds
//!   a plugin VM reports it.
//!
//! ## Refinement, not equality
//!
//! A declaration may NARROW what Rust cannot express: `Table` may be declared
//! `{ success: boolean, stdout: string }`, `Value` may be declared `string`.
//! It may not change arity, swap a primitive, or add or remove optionality.
//! So the shape of a runtime-built table is still a claim nothing verifies —
//! that is this design's floor, and it is why `shell.exec`'s result shape is
//! accepted as written.
//!
//! ## What it still cannot catch
//!
//! Parameter NAMES, because a Rust type does not carry one: `sleep(seconds)`
//! and `sleep(milliseconds)` are the same type. And "raises rather than
//! returning nil", because no type states it. Both remain human care, and
//! both are why the type of a parameter is the cheap half of a signature.

use crate::error::LuaError;
use crate::signature::{LuaType, Param, Signature};
use mlua::{FromLuaMulti, IntoLuaMulti, Lua, MaybeSend, Table, Value};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

/// A Rust type the host can name in Luau.
pub trait LuauValue {
    fn ty() -> LuaType;
}

/// The argument list of a registered function.
pub trait LuauArgs {
    fn arg_types() -> Vec<LuaType>;
}

macro_rules! luau_value {
    ($rust:ty, $ty:expr) => {
        impl LuauValue for $rust {
            fn ty() -> LuaType {
                $ty
            }
        }
    };
}

luau_value!((), LuaType::Nil);
luau_value!(bool, LuaType::Boolean);
luau_value!(i8, LuaType::Number);
luau_value!(i16, LuaType::Number);
luau_value!(i32, LuaType::Number);
luau_value!(i64, LuaType::Number);
luau_value!(u8, LuaType::Number);
luau_value!(u16, LuaType::Number);
luau_value!(u32, LuaType::Number);
luau_value!(u64, LuaType::Number);
luau_value!(usize, LuaType::Number);
luau_value!(f32, LuaType::Number);
luau_value!(f64, LuaType::Number);
luau_value!(String, LuaType::String);
luau_value!(mlua::LuaString, LuaType::String);
luau_value!(mlua::Function, LuaType::Function(Box::new(Signature {
    params: vec![Param {
        name: crate::signature::VARIADIC.to_string(),
        ty: LuaType::Any,
        description: None,
        optional: false,
    }],
    returns: vec![LuaType::Variadic(Box::new(LuaType::Any))],
})));

/// A table with no declared shape. A declaration may narrow it.
impl LuauValue for Table {
    fn ty() -> LuaType {
        LuaType::Map(Box::new(LuaType::String), Box::new(LuaType::Any))
    }
}

/// Anything at all. A declaration may narrow it to whatever it likes.
impl LuauValue for Value {
    fn ty() -> LuaType {
        LuaType::Any
    }
}

impl<T: LuauValue> LuauValue for Option<T> {
    fn ty() -> LuaType {
        LuaType::Optional(Box::new(T::ty()))
    }
}

impl<T: LuauValue> LuauValue for Vec<T> {
    fn ty() -> LuaType {
        LuaType::Array(Box::new(T::ty()))
    }
}

impl<T: LuauValue> LuauValue for HashMap<String, T> {
    fn ty() -> LuaType {
        LuaType::Map(Box::new(LuaType::String), Box::new(T::ty()))
    }
}

/// One argument, mirroring mlua's own blanket `FromLuaMulti` impl. mlua
/// implements `FromLua` for no tuple, so this cannot overlap the tuple impls
/// below — the same coherence mlua relies on.
impl<T: LuauValue> LuauArgs for T {
    fn arg_types() -> Vec<LuaType> {
        match T::ty() {
            // A function of no arguments is written `|_, ()|` in mlua, and
            // `()` there means "nothing", not "one nil argument".
            LuaType::Nil => Vec::new(),
            ty => vec![ty],
        }
    }
}

macro_rules! luau_args_tuple {
    ($($name:ident),+) => {
        impl<$($name: LuauValue),+> LuauArgs for ($($name,)+) {
            fn arg_types() -> Vec<LuaType> {
                vec![$($name::ty()),+]
            }
        }
    };
}

luau_args_tuple!(A, B);
luau_args_tuple!(A, B, C);
luau_args_tuple!(A, B, C, D);
luau_args_tuple!(A, B, C, D, E);
luau_args_tuple!(A, B, C, D, E, F);
luau_args_tuple!(A, B, C, D, E, F, G);
luau_args_tuple!(A, B, C, D, E, F, G, H);

/// Every signature registered on one VM, path → type.
///
/// Held in the VM's app data, so the declarations a generator renders come
/// from the same VM it walked rather than from a table that hopes to match.
#[derive(Clone, Default)]
pub struct HostSignatures(Arc<Mutex<HashMap<String, LuaType>>>);

impl HostSignatures {
    /// The signatures registered on this VM.
    pub fn of(lua: &Lua) -> HostSignatures {
        match lua.app_data_ref::<HostSignatures>() {
            Some(existing) => existing.clone(),
            None => {
                let fresh = HostSignatures::default();
                lua.set_app_data(fresh.clone());
                fresh
            }
        }
    }

    fn record(&self, path: &str, ty: LuaType) {
        if let Ok(mut map) = self.0.lock() {
            map.insert(path.to_string(), ty);
        }
    }

    /// The declared type of one path, if it has one.
    pub fn get(&self, path: &str) -> Option<LuaType> {
        self.0.lock().ok()?.get(path).cloned()
    }

    /// Every declared path.
    pub fn paths(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// A `cru.*` namespace under construction.
pub struct Ns<'lua> {
    lua: &'lua Lua,
    path: String,
    table: Table,
    signatures: HostSignatures,
}

impl<'lua> Ns<'lua> {
    /// Open a namespace at `path` (`"cru.fs"`), creating its table.
    pub fn new(lua: &'lua Lua, path: &str) -> Result<Self, LuaError> {
        Ok(Self {
            lua,
            path: path.to_string(),
            table: lua.create_table()?,
            signatures: HostSignatures::of(lua),
        })
    }

    /// Open a namespace over a table that already exists.
    pub fn over(lua: &'lua Lua, path: &str, table: Table) -> Self {
        Self {
            lua,
            path: path.to_string(),
            table,
            signatures: HostSignatures::of(lua),
        }
    }

    /// The table being built.
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Register a function and declare its type.
    pub fn func<F, A, R>(&mut self, name: &str, decl: &str, f: F) -> Result<(), LuaError>
    where
        F: Fn(&Lua, A) -> mlua::Result<R> + MaybeSend + 'static,
        A: FromLuaMulti + LuauArgs,
        R: IntoLuaMulti + LuauValue,
    {
        let signature = self.check::<A, R>(name, decl)?;
        self.table.set(name, self.lua.create_function(f)?)?;
        self.signatures
            .record(&format!("{}.{name}", self.path), signature);
        Ok(())
    }

    /// Register an async function and declare its type.
    pub fn async_func<F, A, R, FR>(
        &mut self,
        name: &str,
        decl: &str,
        f: F,
    ) -> Result<(), LuaError>
    where
        F: Fn(Lua, A) -> FR + MaybeSend + 'static,
        A: FromLuaMulti + LuauArgs + 'static,
        R: IntoLuaMulti + LuauValue,
        FR: Future<Output = mlua::Result<R>> + MaybeSend + 'static,
    {
        let signature = self.check::<A, R>(name, decl)?;
        self.table
            .set(name, self.lua.create_async_function(f)?)?;
        self.signatures
            .record(&format!("{}.{name}", self.path), signature);
        Ok(())
    }

    /// Declare a member the host provides without a Rust closure here — a
    /// value the daemon sets (`cru.kiln.active`), or a function bound to the
    /// loading plugin (`cru.plugin.publish`).
    ///
    /// Unchecked by construction: there is no Rust type to check against. Use
    /// it only where a closure genuinely cannot carry the declaration.
    pub fn declare_only(&mut self, name: &str, decl: &str) -> Result<(), LuaError> {
        let ty = LuaType::parse(decl).map_err(|e| {
            LuaError::Runtime(format!("{}.{name}: {e}", self.path))
        })?;
        self.signatures.record(&format!("{}.{name}", self.path), ty);
        Ok(())
    }

    /// Publish the table onto `cru`.
    pub fn publish(self) -> Result<Table, LuaError> {
        let leaf = self.path.rsplit('.').next().unwrap_or(&self.path);
        crate::lua_util::register_module(self.lua, leaf, self.table.clone())?;
        Ok(self.table)
    }

    /// Parse the declaration and hold it to the Rust types.
    fn check<A: LuauArgs, R: LuauValue>(
        &self,
        name: &str,
        decl: &str,
    ) -> Result<LuaType, LuaError> {
        let ty = LuaType::parse(decl)
            .map_err(|e| LuaError::Runtime(format!("{}.{name}: {e}", self.path)))?;
        let LuaType::Function(signature) = &ty else {
            return Err(LuaError::Runtime(format!(
                "{}.{name}: the declaration must be a function type, got `{decl}`",
                self.path
            )));
        };

        let expected_args = A::arg_types();
        if signature.params.len() != expected_args.len() {
            return Err(LuaError::Runtime(format!(
                "{}.{name}: the declaration takes {} argument(s), the function takes {}: `{decl}`",
                self.path,
                signature.params.len(),
                expected_args.len()
            )));
        }
        for (param, expected) in signature.params.iter().zip(&expected_args) {
            let declared = if param.optional && !matches!(param.ty, LuaType::Optional(_)) {
                LuaType::Optional(Box::new(param.ty.clone()))
            } else {
                param.ty.clone()
            };
            if !refines(&declared, expected) {
                return Err(LuaError::Runtime(format!(
                    "{}.{name}: parameter `{}` is declared `{}`, the function takes `{}`",
                    self.path,
                    param.name,
                    declared.to_luau(),
                    expected.to_luau()
                )));
            }
        }

        let expected_return = R::ty();
        let declared_return = match signature.returns.len() {
            0 => LuaType::Nil,
            1 => signature.returns[0].clone(),
            _ => LuaType::Any,
        };
        if !refines(&declared_return, &expected_return) {
            return Err(LuaError::Runtime(format!(
                "{}.{name}: the declaration returns `{}`, the function returns `{}`",
                self.path,
                declared_return.to_luau(),
                expected_return.to_luau()
            )));
        }

        Ok(ty)
    }
}

/// Whether `declared` is `rust`, or a narrowing of it.
///
/// Narrowing is allowed in exactly one direction: where Rust says "any table"
/// or "any value", a declaration may say what shape. Everything else must
/// agree — a primitive may not be swapped, and optionality may not be added
/// or dropped, because those are the mismatches that reached plugin authors.
fn refines(declared: &LuaType, rust: &LuaType) -> bool {
    match (declared, rust) {
        // Rust could not say more than "a value" or "a table".
        (_, LuaType::Any) => true,
        (_, LuaType::Map(key, value))
            if **key == LuaType::String && **value == LuaType::Any =>
        {
            // `Table`: any shape, and `nil` is not one of them.
            !matches!(declared, LuaType::Optional(_))
        }
        (LuaType::Optional(inner), LuaType::Optional(expected)) => refines(inner, expected),
        (_, LuaType::Optional(_)) | (LuaType::Optional(_), _) => false,
        (LuaType::Array(inner), LuaType::Array(expected)) => refines(inner, expected),
        (LuaType::Map(dk, dv), LuaType::Map(ek, ev)) => refines(dk, ek) && refines(dv, ev),
        (LuaType::Function(_) | LuaType::Intersection(_), LuaType::Function(_)) => true,
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> Lua {
        let lua = Lua::new();
        crate::lua_util::get_or_create_namespace(&lua, "cru").expect("cru");
        lua
    }

    #[test]
    fn a_matching_declaration_registers() {
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        ns.func("exists", "(path: string) -> boolean", |_, path: String| {
            Ok(!path.is_empty())
        })
        .expect("a matching declaration registers");
        ns.publish().expect("publish");

        let answer: bool = lua.load("return cru.probe.exists('x')").eval().unwrap();
        assert!(answer);
        assert_eq!(
            HostSignatures::of(&lua).get("cru.probe.exists").map(|t| t.to_luau()),
            Some("(path: string) -> boolean".to_string())
        );
    }

    /// The four defect classes this design exists to catch, each as the
    /// mistake that really shipped.
    #[test]
    fn a_wrong_return_is_refused() {
        // `cru.fs.mkdir` was declared `-> boolean`; it returns nothing.
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        let err = ns
            .func("mkdir", "(path: string) -> boolean", |_, _path: String| Ok(()))
            .expect_err("a wrong return must be refused");
        assert!(err.to_string().contains("returns"), "{err}");
    }

    #[test]
    fn a_wrong_arity_is_refused() {
        // `cru.json.encode` takes a second options table.
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        let err = ns
            .func(
                "encode",
                "(value: any) -> string",
                |_, (_value, _opts): (Value, Option<Table>)| Ok(String::new()),
            )
            .expect_err("a wrong arity must be refused");
        assert!(err.to_string().contains("argument(s)"), "{err}");
    }

    #[test]
    fn a_wrong_optionality_is_refused() {
        // `cru.shell.exec`'s `args` was declared optional; `Vec<String>`
        // refuses nil.
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        let err = ns
            .func(
                "exec",
                "(command: string, args: { string }?) -> ()",
                |_, (_cmd, _args): (String, Vec<String>)| Ok(()),
            )
            .expect_err("a wrong optionality must be refused");
        assert!(err.to_string().contains("args"), "{err}");
    }

    #[test]
    fn a_swapped_primitive_is_refused() {
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        let err = ns
            .func("count", "(name: number) -> number", |_, _name: String| Ok(1u32))
            .expect_err("a swapped primitive must be refused");
        assert!(err.to_string().contains("name"), "{err}");
    }

    /// Narrowing is the point: Rust says `Table`, the host says what shape.
    #[test]
    fn a_declaration_may_narrow_a_table_or_a_value() {
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        ns.func(
            "exec",
            "(command: string) -> { success: boolean, stdout: string }",
            |lua, _command: String| lua.create_table(),
        )
        .expect("a table may be narrowed");
        ns.func("which", "(command: string) -> string?", |_, _command: String| {
            Ok(Value::Nil)
        })
        .expect("a value may be narrowed");
    }

    /// A no-argument function is `|_, ()|` in mlua, and `()` means nothing —
    /// not one nil argument.
    #[test]
    fn a_no_argument_function_declares_no_parameters() {
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        ns.func("clock", "() -> number", |_, ()| Ok(1.0f64))
            .expect("a no-argument function registers");
    }

    #[test]
    fn a_declaration_that_is_not_a_function_type_is_refused() {
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        let err = ns
            .func("clock", "number", |_, ()| Ok(1.0f64))
            .expect_err("a non-function declaration must be refused");
        assert!(err.to_string().contains("function type"), "{err}");
    }

    /// The limit, stated as a test: a parameter NAME is not in the Rust type,
    /// so the wrong one registers. `cru.timer.sleep` was declared in
    /// milliseconds for exactly this reason.
    #[test]
    fn a_wrong_parameter_name_still_registers() {
        let lua = vm();
        let mut ns = Ns::new(&lua, "cru.probe").expect("ns");
        ns.func("sleep", "(milliseconds: number) -> ()", |_, _secs: f64| Ok(()))
            .expect("nothing here can see that the name is wrong");
    }
}
