use std::collections::HashMap;

use evalexpr::{Node, Operator, Value};

use super::formula::CONSTANTS;

#[derive(Debug, Clone)]
pub(crate) struct Dual {
    pub v: f64,
    pub d: Vec<f64>,
}

impl Dual {
    pub(crate) fn constant(v: f64, n: usize) -> Self {
        Dual { v, d: vec![0.0; n] }
    }

    pub(crate) fn variable(v: f64, index: usize, n: usize) -> Self {
        let mut d = vec![0.0; n];
        d[index] = 1.0;
        Dual { v, d }
    }

    fn nan(n: usize) -> Self {
        Dual {
            v: f64::NAN,
            d: vec![f64::NAN; n],
        }
    }

    fn chain(&self, v: f64, dv: f64) -> Self {
        Dual {
            v,
            d: self
                .d
                .iter()
                .map(|&x| if x == 0.0 { 0.0 } else { x * dv })
                .collect(),
        }
    }

    fn is_constant(&self) -> bool {
        self.d.iter().all(|&x| x == 0.0)
    }
}

fn mix(a: &Dual, wa: f64, b: &Dual, wb: f64, v: f64) -> Dual {
    let term = |w: f64, x: f64| if x == 0.0 { 0.0 } else { w * x };
    Dual {
        v,
        d: a.d
            .iter()
            .zip(&b.d)
            .map(|(&x, &y)| term(wa, x) + term(wb, y))
            .collect(),
    }
}

fn power(base: &Dual, exponent: &Dual) -> Dual {
    let v = base.v.powf(exponent.v);
    let by_base = if exponent.v == 0.0 {
        0.0
    } else {
        exponent.v * base.v.powf(exponent.v - 1.0)
    };
    if exponent.is_constant() {
        return base.chain(v, by_base);
    }
    let by_exponent = if v == 0.0 { 0.0 } else { base.v.ln() * v };
    mix(base, by_base, exponent, by_exponent, v)
}

pub(crate) fn eval_dual(node: &Node, bindings: &HashMap<&str, Dual>, n: usize) -> Dual {
    let children = node.children();
    let child = |i: usize| {
        children
            .get(i)
            .map_or_else(|| Dual::nan(n), |c| eval_dual(c, bindings, n))
    };
    match node.operator() {
        Operator::RootNode if children.len() == 1 => child(0),
        Operator::Neg => {
            let a = child(0);
            a.chain(-a.v, -1.0)
        }
        Operator::Add => {
            let (a, b) = (child(0), child(1));
            mix(&a, 1.0, &b, 1.0, a.v + b.v)
        }
        Operator::Sub => {
            let (a, b) = (child(0), child(1));
            mix(&a, 1.0, &b, -1.0, a.v - b.v)
        }
        Operator::Mul => {
            let (a, b) = (child(0), child(1));
            mix(&a, b.v, &b, a.v, a.v * b.v)
        }
        Operator::Div => {
            let (a, b) = (child(0), child(1));
            let v = a.v / b.v;
            mix(&a, 1.0 / b.v, &b, -v / b.v, v)
        }
        Operator::Exp => power(&child(0), &child(1)),
        Operator::Const {
            value: Value::Float(v),
        } => Dual::constant(*v, n),
        Operator::Const {
            value: Value::Int(v),
        } => Dual::constant(*v as f64, n),
        Operator::VariableIdentifierRead { identifier } => {
            match CONSTANTS.iter().find(|(name, _)| name == identifier) {
                Some(&(_, v)) => Dual::constant(v, n),
                None => bindings
                    .get(identifier.as_str())
                    .cloned()
                    .unwrap_or_else(|| Dual::nan(n)),
            }
        }
        Operator::FunctionIdentifier { identifier } => {
            let args: Vec<Dual> = arguments(children)
                .into_iter()
                .map(|a| eval_dual(a, bindings, n))
                .collect();
            function(identifier, &args, n)
        }
        _ => Dual::nan(n),
    }
}

fn arguments(children: &[Node]) -> Vec<&Node> {
    let mut node = match children.first() {
        Some(node) => node,
        None => return vec![],
    };
    while matches!(node.operator(), Operator::RootNode) && node.children().len() == 1 {
        node = &node.children()[0];
    }
    match node.operator() {
        Operator::Tuple => node.children().iter().collect(),
        _ => vec![node],
    }
}

fn function(name: &str, args: &[Dual], n: usize) -> Dual {
    let one = |f: fn(f64) -> f64, df: fn(f64) -> f64| match args {
        [a] => a.chain(f(a.v), df(a.v)),
        _ => Dual::nan(n),
    };
    match (name, args) {
        ("math::sqrt", _) => one(f64::sqrt, |x| 0.5 / x.sqrt()),
        ("math::cbrt", _) => one(f64::cbrt, |x| 1.0 / (3.0 * x.cbrt().powi(2))),
        ("math::ln", _) => one(f64::ln, |x| 1.0 / x),
        ("math::log10", _) => one(f64::log10, |x| 1.0 / (x * std::f64::consts::LN_10)),
        ("math::log2", _) => one(f64::log2, |x| 1.0 / (x * std::f64::consts::LN_2)),
        ("math::exp", _) => one(f64::exp, f64::exp),
        ("math::exp2", _) => one(f64::exp2, |x| x.exp2() * std::f64::consts::LN_2),
        ("math::abs", _) => one(f64::abs, |x| if x == 0.0 { 0.0 } else { x.signum() }),
        ("math::sin", _) => one(f64::sin, f64::cos),
        ("math::cos", _) => one(f64::cos, |x| -x.sin()),
        ("math::tan", _) => one(f64::tan, |x| 1.0 / x.cos().powi(2)),
        ("math::asin", _) => one(f64::asin, |x| 1.0 / (1.0 - x * x).sqrt()),
        ("math::acos", _) => one(f64::acos, |x| -1.0 / (1.0 - x * x).sqrt()),
        ("math::atan", _) => one(f64::atan, |x| 1.0 / (1.0 + x * x)),
        ("math::sinh", _) => one(f64::sinh, f64::cosh),
        ("math::cosh", _) => one(f64::cosh, f64::sinh),
        ("math::tanh", _) => one(f64::tanh, |x| 1.0 - x.tanh().powi(2)),
        ("math::asinh", _) => one(f64::asinh, |x| 1.0 / (x * x + 1.0).sqrt()),
        ("math::acosh", _) => one(f64::acosh, |x| 1.0 / (x * x - 1.0).sqrt()),
        ("math::atanh", _) => one(f64::atanh, |x| 1.0 / (1.0 - x * x)),
        ("math::pow", [base, exponent]) => power(base, exponent),
        ("math::hypot", [a, b]) => {
            let v = a.v.hypot(b.v);
            mix(a, a.v / v, b, b.v / v, v)
        }
        ("math::atan2", [y, x]) => {
            let r2 = x.v * x.v + y.v * y.v;
            mix(y, x.v / r2, x, -y.v / r2, y.v.atan2(x.v))
        }
        ("math::log", [x, base]) => {
            let ln_base = base.v.ln();
            let v = x.v.ln() / ln_base;
            if base.is_constant() {
                return x.chain(v, 1.0 / (x.v * ln_base));
            }
            mix(x, 1.0 / (x.v * ln_base), base, -v / (base.v * ln_base), v)
        }
        ("min", [first, rest @ ..]) => rest
            .iter()
            .fold(first, |best, a| if a.v < best.v { a } else { best })
            .clone(),
        ("max", [first, rest @ ..]) => rest
            .iter()
            .fold(first, |best, a| if a.v > best.v { a } else { best })
            .clone(),
        _ => Dual::nan(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computation::formula::{eval_compiled, parse};

    fn at(formula: &str, point: &[(&str, f64)]) -> (Dual, f64, Vec<f64>) {
        let tree = parse(formula).unwrap();
        let n = point.len();
        let bindings: HashMap<&str, Dual> = point
            .iter()
            .enumerate()
            .map(|(i, &(s, v))| (s, Dual::variable(v, i, n)))
            .collect();
        let dual = eval_dual(&tree, &bindings, n);
        let plain = |values: &[f64]| {
            let map: HashMap<&str, f64> = point
                .iter()
                .map(|&(s, _)| s)
                .zip(values.iter().copied())
                .collect();
            eval_compiled(&tree, &map)
        };
        let means: Vec<f64> = point.iter().map(|&(_, v)| v).collect();
        let numeric = (0..n)
            .map(|i| {
                let h = 1e-6 * means[i].abs().max(1.0);
                let mut x = means.clone();
                x[i] += h;
                let up = plain(&x);
                x[i] -= 2.0 * h;
                (up - plain(&x)) / (2.0 * h)
            })
            .collect();
        (dual, plain(&means), numeric)
    }

    #[test]
    fn every_function_matches_its_value_and_a_numerical_derivative() {
        let cases: &[(&str, &[(&str, f64)])] = &[
            ("a + b*c - a/b", &[("a", 1.3), ("b", 0.7), ("c", 2.1)]),
            ("-a^b", &[("a", 1.3), ("b", 0.7)]),
            ("a^2", &[("a", -1.3)]),
            ("math::sqrt(a)", &[("a", 2.0)]),
            ("math::cbrt(a)", &[("a", 2.0)]),
            ("math::ln(a)", &[("a", 2.0)]),
            ("math::log10(a)", &[("a", 2.0)]),
            ("math::log2(a)", &[("a", 2.0)]),
            ("math::exp(a)", &[("a", 0.4)]),
            ("math::exp2(a)", &[("a", 0.4)]),
            ("math::abs(a)", &[("a", -0.4)]),
            ("math::sin(a)", &[("a", 0.4)]),
            ("math::cos(a)", &[("a", 0.4)]),
            ("math::tan(a)", &[("a", 0.4)]),
            ("math::asin(a)", &[("a", 0.4)]),
            ("math::acos(a)", &[("a", 0.4)]),
            ("math::atan(a)", &[("a", 0.4)]),
            ("math::sinh(a)", &[("a", 0.4)]),
            ("math::cosh(a)", &[("a", 0.4)]),
            ("math::tanh(a)", &[("a", 0.4)]),
            ("math::asinh(a)", &[("a", 0.4)]),
            ("math::acosh(a)", &[("a", 1.4)]),
            ("math::atanh(a)", &[("a", 0.4)]),
            ("math::pow(a, b)", &[("a", 1.3), ("b", 0.7)]),
            ("math::hypot(a, b)", &[("a", 1.3), ("b", 0.7)]),
            ("math::atan2(a, b)", &[("a", 1.3), ("b", -0.7)]),
            ("math::log(a, b)", &[("a", 5.0), ("b", 3.0)]),
            ("min(a, b, c)", &[("a", 1.3), ("b", 0.7), ("c", 2.1)]),
            ("max(a, b, c)", &[("a", 1.3), ("b", 0.7), ("c", 2.1)]),
            ("2*pi*a + e", &[("a", 1.3)]),
        ];
        for (formula, point) in cases {
            let (dual, value, numeric) = at(formula, point);
            assert_eq!(dual.v, value, "{formula}");
            for (exact, approx) in dual.d.iter().zip(&numeric) {
                assert!(
                    (exact - approx).abs() < 1e-6 * approx.abs().max(1.0),
                    "{formula}: {exact} vs {approx}"
                );
            }
        }
    }

    fn u_of(
        formula: &str,
        point: &[(&str, f64)],
        us: &[f64],
        factors: &[Vec<(usize, f64)>],
    ) -> f64 {
        let (dual, _, _) = at(formula, point);
        crate::uncertainty::propagated_u(&dual.d, us, factors)
    }

    fn relative(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs()
    }

    #[test]
    fn exact_derivatives_where_finite_differences_struggled() {
        assert!(
            relative(
                u_of("a + b", &[("a", 1e-17), ("b", 1.0)], &[0.3, 0.4], &[]),
                0.5
            ) < 1e-12
        );
        assert!(relative(u_of("math::sqrt(a)", &[("a", 1e-8)], &[0.1], &[]), 500.0) < 1e-12);
        assert_eq!(u_of("1e12 + (a - 1)^3", &[("a", 1.0)], &[0.01], &[]), 0.0);
        assert!(relative(u_of("1 + a", &[("a", 0.0)], &[1e-12], &[]), 1e-12) < 1e-12);
        assert!(relative(u_of("1/a", &[("a", 1e-8)], &[0.1], &[]), 1e15) < 1e-12);
        assert!(relative(u_of("1/a", &[("a", 1e-8)], &[1e-10], &[]), 1e6) < 1e-12);
        let shared = [vec![(0, 0.1), (1, -0.3)]];
        assert!(
            relative(
                u_of(
                    "(a - 1) + b",
                    &[("a", 1.0), ("b", 1e-17)],
                    &[0.0, 0.0],
                    &shared
                ),
                0.2
            ) < 1e-12
        );
        let (m, b): (f64, f64) = (0.997, 1.004);
        let tiny = [vec![(0, 0.005), (1, -1e-12)]];
        let analytic = ((2.0 - b) / (m * m) * 0.005 - 1e-12 / m).abs();
        assert!(
            relative(
                u_of("(2 - b)/m", &[("m", m), ("b", b)], &[0.0, 0.0], &tiny),
                analytic
            ) < 1e-12
        );
    }

    #[test]
    fn a_constant_exponent_does_not_need_the_log_of_the_base() {
        let (dual, _, _) = at("a^2", &[("a", -1.5)]);
        assert_eq!(dual.d, vec![-3.0]);
    }

    #[test]
    fn powers_at_a_zero_base_take_their_limits() {
        for formula in ["a^0", "math::pow(a, 0)"] {
            let (dual, _, _) = at(formula, &[("a", 0.0)]);
            assert_eq!(dual.d, vec![0.0], "{formula}");
        }
        for formula in ["a^b", "math::pow(a, b)"] {
            let (dual, _, _) = at(formula, &[("a", 0.0), ("b", 2.0)]);
            assert_eq!(dual.d, vec![0.0, 0.0], "{formula}");
            let (dual, _, _) = at(formula, &[("a", 3.0), ("b", 0.0)]);
            assert_eq!(dual.d, vec![0.0, 3.0_f64.ln()], "{formula}");
        }
    }

    #[test]
    fn unknown_operators_give_nan() {
        let (dual, _, _) = at("a % 2", &[("a", 3.0)]);
        assert!(dual.v.is_nan());
    }
}
