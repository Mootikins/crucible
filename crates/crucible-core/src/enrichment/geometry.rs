//! Vector geometry for retrieval over block embeddings.
//!
//! Every function here is pure. A strategy in Lua composes them through
//! `cru.vec`, so the numeric work stays in Rust and the strategy stays a
//! table.
//!
//! Scores are `f64`, which is what a `SearchResult` carries and what Lua
//! reads. Vectors stay `f32`, which is what the store holds.
//!
//! # The arc
//!
//! Two adjacent blocks `a` and `b` of one note are two points on the unit
//! sphere. The great-circle arc between them holds every unit vector that
//! is a mix of the two. [`arc_best`] answers the best cosine a query can
//! reach on that arc, from the three cosines `q·a`, `q·b` and `a·b` alone.
//!
//! Let `c = a·b`, `Ω = arccos(c)`, and build an orthonormal basis of the
//! plane: `e1 = a`, `e2 = (b - c·a) / sqrt(1 - c²)`. The query's
//! coordinates in that plane are `u = q·a` and
//! `v = (q·b - c·(q·a)) / sqrt(1 - c²)`, and the angle of its projection
//! is `φ = atan2(v, u)`. When `0 ≤ φ ≤ Ω` the projection falls inside the
//! arc, and the best cosine is `sqrt(u² + v²)` at `t = φ / Ω`. Otherwise an
//! endpoint wins.

/// The dot product of two vectors, accumulated in `f64`.
///
/// A length mismatch is a caller bug. The shorter length is used, so the
/// answer is at least defined.
pub fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum()
}

/// The Euclidean norm of a vector.
pub fn norm(v: &[f32]) -> f64 {
    dot(v, v).sqrt()
}

/// The unit vector in the direction of `v`.
///
/// A zero vector normalizes to itself. Nothing can point along it.
pub fn normalize(v: &[f32]) -> Vec<f32> {
    let n = norm(v);
    if n == 0.0 {
        return v.to_vec();
    }
    v.iter().map(|x| (f64::from(*x) / n) as f32).collect()
}

/// The cosine of the angle between two vectors.
///
/// A zero vector has no direction, so its cosine with anything is `0.0`.
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let denominator = norm(a) * norm(b);
    if denominator == 0.0 {
        return 0.0;
    }
    dot(a, b) / denominator
}

/// Below this, `a` and `b` point the same way, or opposite ways, and the
/// arc between them is a point or is undefined.
const DEGENERATE_ARC: f64 = 1e-12;

/// The best cosine between `q` and any point on the great-circle arc from
/// `a` to `b`, and where on the arc it is.
///
/// Returns `(score, t)`. `t = 0` is `a`, `t = 1` is `b`, and a value
/// between is the slerp parameter of the best point. Inputs need not be
/// unit vectors; the function normalizes.
pub fn arc_best(q: &[f32], a: &[f32], b: &[f32]) -> (f64, f64) {
    let ca = cosine(q, a);
    let cb = cosine(q, b);
    let c = cosine(a, b).clamp(-1.0, 1.0);

    let endpoint = || if ca >= cb { (ca, 0.0) } else { (cb, 1.0) };

    let sin2 = 1.0 - c * c;
    if sin2 <= DEGENERATE_ARC {
        return endpoint();
    }
    let sin = sin2.sqrt();
    let u = ca;
    let v = (cb - c * ca) / sin;
    let phi = v.atan2(u);
    let omega = c.acos();

    if phi < 0.0 || phi > omega {
        return endpoint();
    }
    ((u * u + v * v).sqrt(), phi / omega)
}

/// `n` points on the Bézier curve whose control points are `points`,
/// evaluated by de Casteljau in the ambient space, each normalized.
///
/// The samples run from `t = 0` to `t = 1` inclusive. One sample is the
/// first control point. No control points give no samples.
pub fn bezier_samples(points: &[Vec<f32>], n: usize) -> Vec<Vec<f32>> {
    if points.is_empty() || n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            let t = if n == 1 {
                0.0
            } else {
                i as f64 / (n - 1) as f64
            };
            normalize(&de_casteljau(points, t))
        })
        .collect()
}

/// One point of the Bézier curve at parameter `t`.
fn de_casteljau(points: &[Vec<f32>], t: f64) -> Vec<f32> {
    let mut layer: Vec<Vec<f64>> = points
        .iter()
        .map(|p| p.iter().map(|x| f64::from(*x)).collect())
        .collect();
    while layer.len() > 1 {
        layer = layer
            .windows(2)
            .map(|pair| {
                pair[0]
                    .iter()
                    .zip(pair[1].iter())
                    .map(|(x, y)| x + (y - x) * t)
                    .collect()
            })
            .collect();
    }
    layer
        .pop()
        .unwrap_or_default()
        .into_iter()
        .map(|x| x as f32)
        .collect()
}

/// The best cosine between `q` and any of `n` samples of the Bézier curve
/// through `points`, and the parameter `t` of that sample.
///
/// With no control points, or `n = 0`, there is nothing to score:
/// the answer is `(0.0, 0.0)`.
pub fn curve_best(q: &[f32], points: &[Vec<f32>], n: usize) -> (f64, f64) {
    let samples = bezier_samples(points, n);
    let count = samples.len();
    samples
        .iter()
        .enumerate()
        .map(|(i, sample)| {
            let t = if count == 1 {
                0.0
            } else {
                i as f64 / (count - 1) as f64
            };
            (cosine(q, sample), t)
        })
        .fold((0.0, 0.0), |best, candidate| {
            if candidate.0 > best.0 {
                candidate
            } else {
                best
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f64 = 1e-5;

    /// A small deterministic generator, so a failure repeats.
    struct XorShift(u64);

    impl XorShift {
        fn next_f64(&mut self) -> f64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            (x >> 11) as f64 / (1u64 << 53) as f64
        }

        /// A random point on the unit sphere, near enough: uniform
        /// coordinates in `[-1, 1]`, normalized.
        fn unit_vector(&mut self, dims: usize) -> Vec<f32> {
            let raw: Vec<f32> = (0..dims)
                .map(|_| (self.next_f64() * 2.0 - 1.0) as f32)
                .collect();
            normalize(&raw)
        }
    }

    /// Spherical linear interpolation, the reference the closed form is
    /// checked against.
    fn slerp(a: &[f32], b: &[f32], t: f64) -> Vec<f32> {
        let omega = cosine(a, b).clamp(-1.0, 1.0).acos();
        let sin = omega.sin();
        let wa = ((1.0 - t) * omega).sin() / sin;
        let wb = (t * omega).sin() / sin;
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (wa * f64::from(*x) + wb * f64::from(*y)) as f32)
            .collect()
    }

    fn brute_force_arc(q: &[f32], a: &[f32], b: &[f32], samples: usize) -> (f64, f64) {
        (0..=samples)
            .map(|i| {
                let t = i as f64 / samples as f64;
                (cosine(q, &slerp(a, b, t)), t)
            })
            .fold(
                (f64::MIN, 0.0),
                |best, c| if c.0 > best.0 { c } else { best },
            )
    }

    #[test]
    fn dot_and_cosine_match_hand_values() {
        assert_eq!(dot(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]), 32.0);
        assert!(
            (cosine(&[1.0, 0.0], &[1.0, 1.0]) - std::f64::consts::FRAC_1_SQRT_2).abs() < TOLERANCE
        );
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn normalize_gives_a_unit_vector_and_keeps_zero() {
        let unit = normalize(&[3.0, 4.0]);
        assert!((norm(&unit) - 1.0).abs() < TOLERANCE);
        assert!((f64::from(unit[0]) - 0.6).abs() < TOLERANCE);
        assert_eq!(normalize(&[0.0, 0.0]), vec![0.0, 0.0]);
    }

    #[test]
    fn arc_best_matches_a_brute_force_sweep_on_random_unit_vectors() {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for _ in 0..200 {
            let q = rng.unit_vector(16);
            let a = rng.unit_vector(16);
            let b = rng.unit_vector(16);
            let (score, t) = arc_best(&q, &a, &b);
            let (reference, t_ref) = brute_force_arc(&q, &a, &b, 1000);
            assert!(
                (score - reference).abs() < TOLERANCE,
                "closed form {score} vs sweep {reference} (t {t} vs {t_ref})"
            );
            assert!((0.0..=1.0).contains(&t), "t out of range: {t}");
            assert!((t - t_ref).abs() < 2e-3, "t {t} vs sweep {t_ref}");
        }
    }

    #[test]
    fn arc_best_reaches_the_endpoints_at_t_zero_and_one() {
        let a = normalize(&[1.0, 0.0, 0.0]);
        let b = normalize(&[0.0, 1.0, 0.0]);

        // A query beyond `a` on the far side of the arc: `a` wins.
        let (score, t) = arc_best(&normalize(&[1.0, -0.5, 0.0]), &a, &b);
        assert_eq!(t, 0.0);
        assert!((score - cosine(&[1.0, -0.5, 0.0], &a)).abs() < TOLERANCE);

        // A query beyond `b`: `b` wins.
        let (score, t) = arc_best(&normalize(&[-0.5, 1.0, 0.0]), &a, &b);
        assert_eq!(t, 1.0);
        assert!((score - cosine(&[-0.5, 1.0, 0.0], &b)).abs() < TOLERANCE);
    }

    /// The research's 1.41 factor: orthogonal endpoints each score
    /// `sqrt(2)/2` against a query at their normalized mean, and the arc
    /// scores `1.0`, which is `sqrt(2)` times more.
    #[test]
    fn arc_best_beats_orthogonal_endpoints_by_root_two_at_their_mean() {
        let a = vec![1.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0];
        let q = normalize(&[1.0, 1.0, 0.0, 0.0]);

        let endpoint = cosine(&q, &a);
        assert!((endpoint - std::f64::consts::FRAC_1_SQRT_2).abs() < TOLERANCE);

        let (score, t) = arc_best(&q, &a, &b);
        assert!((score - 1.0).abs() < TOLERANCE, "arc score {score}");
        assert!((t - 0.5).abs() < TOLERANCE, "t {t}");
        assert!((score / endpoint - std::f64::consts::SQRT_2).abs() < TOLERANCE);
    }

    #[test]
    fn arc_best_with_equal_endpoints_answers_the_point() {
        let a = normalize(&[0.3, 0.4, 0.5]);
        let q = normalize(&[0.1, 0.9, 0.2]);
        let (score, t) = arc_best(&q, &a, &a);
        assert!((score - cosine(&q, &a)).abs() < TOLERANCE);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn bezier_samples_start_and_end_at_the_control_endpoints() {
        let points = vec![
            normalize(&[1.0, 0.0, 0.0]),
            normalize(&[1.0, 1.0, 0.0]),
            normalize(&[0.0, 1.0, 0.0]),
        ];
        let samples = bezier_samples(&points, 5);
        assert_eq!(samples.len(), 5);
        assert!((cosine(&samples[0], &points[0]) - 1.0).abs() < TOLERANCE);
        assert!((cosine(&samples[4], &points[2]) - 1.0).abs() < TOLERANCE);
        for sample in &samples {
            assert!((norm(sample) - 1.0).abs() < TOLERANCE);
        }
        assert!(bezier_samples(&[], 5).is_empty());
        assert_eq!(bezier_samples(&points, 1), vec![points[0].clone()]);
    }

    #[test]
    fn curve_best_finds_the_middle_of_a_bent_curve() {
        let points = vec![
            vec![1.0, 0.0, 0.0],
            vec![1.0, 1.0, 0.0],
            vec![0.0, 1.0, 0.0],
        ];
        // The query points along the curve's middle control point.
        let q = normalize(&[1.0, 1.0, 0.0]);
        let (score, t) = curve_best(&q, &points, 101);
        assert!((score - 1.0).abs() < TOLERANCE, "score {score}");
        assert!((t - 0.5).abs() < TOLERANCE, "t {t}");

        let (end_score, end_t) = curve_best(&[0.0, 1.0, 0.0], &points, 101);
        assert!((end_score - 1.0).abs() < TOLERANCE);
        assert_eq!(end_t, 1.0);

        assert_eq!(curve_best(&q, &[], 10), (0.0, 0.0));
    }
}
