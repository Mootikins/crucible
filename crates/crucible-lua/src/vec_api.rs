//! `cru.vec`: vector geometry over plain Lua number arrays.
//!
//! A retrieval strategy in Lua composes these; the arithmetic itself is
//! `crucible_core::enrichment::geometry`, so a strategy and the daemon's
//! own scoring cannot disagree on a number.
//!
//! Vectors cross as plain arrays. At a few hundred floats and tens of hits
//! per call, that is well inside a stage budget. Measure before reaching
//! for a userdata handle.

use crate::error::LuaError;
use crucible_core::enrichment::geometry;
use mlua::Lua;

/// The sample count `curve_best` uses when Lua names none.
const DEFAULT_CURVE_SAMPLES: usize = 32;

/// Refuse two vectors of different lengths, by function name and lengths.
///
/// `geometry` itself takes the shorter length. A strategy that mixes a
/// 384-float vector with a 768-float one has a bug, and the number it would
/// get back says nothing about it.
fn same_length(function: &str, a: &[f32], b: &[f32]) -> mlua::Result<()> {
    if a.len() == b.len() {
        return Ok(());
    }
    Err(mlua::Error::runtime(format!(
        "cru.vec.{function}: the vectors differ in length ({} and {})",
        a.len(),
        b.len()
    )))
}

/// Register `cru.vec` on the VM.
pub fn register_vec_module(lua: &Lua) -> Result<(), LuaError> {
    let mut vec = crate::host_registry::Ns::new(lua, "cru.vec")?;

    vec.func(
        "dot",
        "(a: { number }, b: { number }) -> number",
        |_, (a, b): (Vec<f32>, Vec<f32>)| {
            same_length("dot", &a, &b)?;
            Ok(geometry::dot(&a, &b))
        },
    )?;
    vec.doc("dot", "The dot product of two vectors of one length.");

    vec.func(
        "normalize",
        "(v: { number }) -> { number }",
        |_, v: Vec<f32>| Ok(geometry::normalize(&v)),
    )?;
    vec.doc(
        "normalize",
        "The unit vector in the direction of `v`. A zero vector stays zero.",
    );

    vec.func(
        "cosine",
        "(a: { number }, b: { number }) -> number",
        |_, (a, b): (Vec<f32>, Vec<f32>)| {
            same_length("cosine", &a, &b)?;
            Ok(geometry::cosine(&a, &b))
        },
    )?;
    vec.doc(
        "cosine",
        "The cosine of the angle between two vectors. A zero vector scores 0.",
    );

    vec.func(
        "arc_best",
        "(q: { number }, a: { number }, b: { number }) -> (number, number)",
        |_, (q, a, b): (Vec<f32>, Vec<f32>, Vec<f32>)| {
            same_length("arc_best", &q, &a)?;
            same_length("arc_best", &a, &b)?;
            Ok(geometry::arc_best(&q, &a, &b))
        },
    )?;
    vec.doc(
        "arc_best",
        "The best cosine between `q` and any point on the great-circle arc \
         from `a` to `b`, then where on the arc: `t = 0` is `a`, `t = 1` is `b`.",
    );

    vec.func(
        "curve_best",
        "(q: { number }, points: { { number } }, n: number?) -> (number, number)",
        |_, (q, points, n): (Vec<f32>, Vec<Vec<f32>>, Option<usize>)| {
            for point in &points {
                same_length("curve_best", &q, point)?;
            }
            Ok(geometry::curve_best(
                &q,
                &points,
                n.unwrap_or(DEFAULT_CURVE_SAMPLES),
            ))
        },
    )?;
    vec.doc(
        "curve_best",
        "The best cosine between `q` and `n` samples (default 32) of the \
         Bezier curve through `points`, then the `t` of that sample.",
    );

    vec.publish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::LuaExecutor;

    const TOLERANCE: f64 = 1e-5;

    fn lua() -> LuaExecutor {
        LuaExecutor::new().expect("executor")
    }

    #[test]
    fn dot_normalize_and_cosine_round_trip_from_lua() {
        let executor = lua();
        let lua = executor.lua();

        let dot: f64 = lua
            .load("return cru.vec.dot({1, 2, 3}, {4, 5, 6})")
            .eval()
            .expect("dot");
        assert_eq!(dot, 32.0);

        let unit: Vec<f64> = lua
            .load("return cru.vec.normalize({3, 4})")
            .eval()
            .expect("normalize");
        assert!((unit[0] - 0.6).abs() < TOLERANCE);
        assert!((unit[1] - 0.8).abs() < TOLERANCE);

        let cosine: f64 = lua
            .load("return cru.vec.cosine({1, 0}, {1, 1})")
            .eval()
            .expect("cosine");
        assert!((cosine - std::f64::consts::FRAC_1_SQRT_2).abs() < TOLERANCE);
    }

    #[test]
    fn arc_best_answers_the_score_and_the_position() {
        let executor = lua();
        let (score, t): (f64, f64) = executor
            .lua()
            .load("return cru.vec.arc_best({1, 1, 0}, {1, 0, 0}, {0, 1, 0})")
            .eval()
            .expect("arc_best");
        assert!((score - 1.0).abs() < TOLERANCE, "score {score}");
        assert!((t - 0.5).abs() < TOLERANCE, "t {t}");
    }

    #[test]
    fn curve_best_takes_a_list_of_points_and_an_optional_sample_count() {
        let executor = lua();
        let lua = executor.lua();
        let (score, t): (f64, f64) = lua
            .load("return cru.vec.curve_best({1, 1, 0}, { {1, 0, 0}, {1, 1, 0}, {0, 1, 0} }, 101)")
            .eval()
            .expect("curve_best");
        assert!((score - 1.0).abs() < TOLERANCE, "score {score}");
        assert!((t - 0.5).abs() < TOLERANCE, "t {t}");

        let (score, _): (f64, f64) = lua
            .load("return cru.vec.curve_best({0, 1, 0}, { {1, 0, 0}, {0, 1, 0} })")
            .eval()
            .expect("curve_best with the default sample count");
        assert!((score - 1.0).abs() < TOLERANCE, "score {score}");
    }

    #[test]
    fn a_length_mismatch_is_refused_by_name() {
        let executor = lua();
        let err = executor
            .lua()
            .load("return cru.vec.dot({1, 2}, {1})")
            .eval::<f64>()
            .expect_err("mismatched lengths must raise");
        let text = err.to_string();
        assert!(
            text.contains("cru.vec.dot") && text.contains('2') && text.contains('1'),
            "unhelpful: {text}"
        );
    }

    /// Every `cru.vec` function carries a checked signature, so the
    /// declarations describe it.
    #[test]
    fn every_vec_function_is_signed() {
        let executor = lua();
        let signatures = crate::HostSignatures::of(executor.lua());
        for name in ["dot", "normalize", "cosine", "arc_best", "curve_best"] {
            assert!(
                signatures.get(&format!("cru.vec.{name}")).is_some(),
                "cru.vec.{name} has no declared signature"
            );
        }
    }
}
