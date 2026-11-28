use evalexpr::{Node, Operator, Value};

pub type Dims = [i8; 7];

pub const NONE: Dims = [0; 7];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dim {
    Number,
    Known(Dims),
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inference {
    pub dim: Dim,
    pub mixed: Option<(Dims, Dims)>,
    pub argument: Option<(String, Dims)>,
}

pub fn of_unit(unit: &str) -> Dim {
    match crate::units::parse(unit) {
        Some(u) => Dim::Known(u.dims[..7].try_into().unwrap_or(NONE)),
        None => Dim::Unknown,
    }
}

/// Las dimensiones de una fórmula, y si suma dimensiones distintas o le pasa dimensiones a ln, exp o un seno.
///
/// ```
/// use quantify::dimensions::{infer, of_unit, Dim};
/// let dims = |s: &str| match s { "L" => of_unit("m"), "T" => of_unit("s"), _ => Dim::Unknown };
/// assert_eq!(infer("4*pi^2*L/T^2", &dims).dim, of_unit("m/s2"));
/// assert!(infer("L + T", &dims).mixed.is_some());
/// assert!(infer("math::ln(T)", &dims).argument.is_some());
/// ```
pub fn infer(formula: &str, dims_of: &dyn Fn(&str) -> Dim) -> Inference {
    let mut out = Inference {
        dim: Dim::Unknown,
        mixed: None,
        argument: None,
    };
    if let Ok(tree) = crate::computation::parse(formula) {
        out.dim = walk(&tree, dims_of, &mut out);
    }
    out
}

pub fn text(dims: Dims) -> String {
    const NAMES: [(usize, &str); 7] = [
        (1, "kg"),
        (0, "m"),
        (2, "s"),
        (3, "A"),
        (4, "K"),
        (5, "mol"),
        (6, "cd"),
    ];
    let parts: Vec<String> = NAMES
        .iter()
        .filter(|(i, _)| dims[*i] != 0)
        .map(|(i, name)| match dims[*i] {
            1 => (*name).to_string(),
            e => format!("{name}{}", superscript(e)),
        })
        .collect();
    if parts.is_empty() {
        "adimensional".into()
    } else {
        parts.join("·")
    }
}

fn superscript(e: i8) -> String {
    e.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

fn walk(node: &Node, dims_of: &dyn Fn(&str) -> Dim, out: &mut Inference) -> Dim {
    let children = node.children();
    let child = |i: usize, out: &mut Inference| {
        children
            .get(i)
            .map_or(Dim::Unknown, |c| walk(c, dims_of, out))
    };
    match node.operator() {
        Operator::RootNode if children.len() == 1 => child(0, out),
        Operator::Neg => child(0, out),
        Operator::Add | Operator::Sub => {
            let (a, b) = (child(0, out), child(1, out));
            same(a, b, out)
        }
        Operator::Mul => combine(child(0, out), child(1, out), 1),
        Operator::Div => combine(child(0, out), child(1, out), -1),
        Operator::Exp => {
            let base = child(0, out);
            let exponent = child(1, out);
            dimensionless("el exponente", exponent, out);
            power(base, children.get(1).and_then(constant))
        }
        Operator::Const {
            value: Value::Int(_) | Value::Float(_),
        } => Dim::Number,
        Operator::VariableIdentifierRead { identifier } => {
            if crate::computation::CONSTANTS
                .iter()
                .any(|(name, _)| name == identifier)
            {
                Dim::Number
            } else {
                dims_of(identifier)
            }
        }
        Operator::FunctionIdentifier { identifier } => {
            let args = arguments(children);
            function(identifier, &args, dims_of, out)
        }
        _ => Dim::Unknown,
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

fn function(name: &str, args: &[&Node], dims_of: &dyn Fn(&str) -> Dim, out: &mut Inference) -> Dim {
    let arg =
        |i: usize, out: &mut Inference| args.get(i).map_or(Dim::Unknown, |a| walk(a, dims_of, out));
    match name {
        "math::sqrt" => power(arg(0, out), Some(0.5)),
        "math::cbrt" => power(arg(0, out), Some(1.0 / 3.0)),
        "math::pow" => {
            let base = arg(0, out);
            let exponent = arg(1, out);
            dimensionless("el exponente", exponent, out);
            power(base, args.get(1).and_then(|n| constant(n)))
        }
        "math::abs" => arg(0, out),
        "min" | "max" | "math::hypot" => {
            let first = arg(0, out);
            (1..args.len()).fold(first, |acc, i| {
                let next = arg(i, out);
                same(acc, next, out)
            })
        }
        "math::atan2" => {
            let (y, x) = (arg(0, out), arg(1, out));
            same(y, x, out);
            Dim::Known(NONE)
        }
        "math::ln" | "math::log10" | "math::log2" | "math::log" | "math::exp" | "math::exp2"
        | "math::sin" | "math::cos" | "math::tan" | "math::asin" | "math::acos" | "math::atan"
        | "math::sinh" | "math::cosh" | "math::tanh" | "math::asinh" | "math::acosh"
        | "math::atanh" => {
            let what = format!("el argumento de {}", name.trim_start_matches("math::"));
            for i in 0..args.len() {
                let dim = arg(i, out);
                dimensionless(&what, dim, out);
            }
            Dim::Known(NONE)
        }
        _ => Dim::Unknown,
    }
}

fn dimensionless(what: &str, dim: Dim, out: &mut Inference) {
    if let Dim::Known(d) = dim {
        if d != NONE && out.argument.is_none() {
            out.argument = Some((what.to_string(), d));
        }
    }
}

fn same(a: Dim, b: Dim, out: &mut Inference) -> Dim {
    match (a, b) {
        (Dim::Number, x) | (x, Dim::Number) => x,
        (Dim::Known(x), Dim::Known(y)) if x == y => Dim::Known(x),
        (Dim::Known(x), Dim::Known(y)) => {
            out.mixed.get_or_insert((x, y));
            Dim::Unknown
        }
        _ => Dim::Unknown,
    }
}

pub fn divide(a: Dim, b: Dim) -> Dim {
    combine(a, b, -1)
}

fn combine(a: Dim, b: Dim, sign: i8) -> Dim {
    let b = match b {
        Dim::Known(d) => {
            let mut scaled = NONE;
            for i in 0..7 {
                match d[i].checked_mul(sign) {
                    Some(e) => scaled[i] = e,
                    None => return Dim::Unknown,
                }
            }
            Dim::Known(scaled)
        }
        other => other,
    };
    match (a, b) {
        (Dim::Number, x) | (x, Dim::Number) => x,
        (Dim::Known(x), Dim::Known(y)) => {
            let mut sum = NONE;
            for i in 0..7 {
                sum[i] = match x[i].checked_add(y[i]) {
                    Some(e) => e,
                    None => return Dim::Unknown,
                };
            }
            Dim::Known(sum)
        }
        _ => Dim::Unknown,
    }
}

fn power(base: Dim, exponent: Option<f64>) -> Dim {
    match base {
        Dim::Number => Dim::Number,
        Dim::Known(d) if d == NONE => Dim::Known(NONE),
        Dim::Known(d) => {
            let Some(n) = exponent else {
                return Dim::Unknown;
            };
            let mut out = NONE;
            for i in 0..7 {
                let e = f64::from(d[i]) * n;
                if !e.is_finite() || (e - e.round()).abs() > 1e-9 || e.abs() > 127.0 {
                    return Dim::Unknown;
                }
                out[i] = e.round() as i8;
            }
            Dim::Known(out)
        }
        Dim::Unknown => Dim::Unknown,
    }
}

fn constant(node: &Node) -> Option<f64> {
    if node.iter_variable_identifiers().next().is_some() {
        return None;
    }
    node.eval().ok()?.as_number().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims(symbol: &str) -> Dim {
        match symbol {
            "L" => of_unit("m"),
            "T" => of_unit("s"),
            "K" => of_unit("K"),
            "f" => of_unit("Hz"),
            "w" => of_unit("rad/s"),
            "phi" => of_unit("rad"),
            "a" => of_unit("div"),
            "x" => of_unit(""),
            _ => Dim::Unknown,
        }
    }

    fn dim(formula: &str) -> Dim {
        infer(formula, &dims).dim
    }

    #[test]
    fn products_and_powers_add_and_scale_exponents() {
        assert_eq!(dim("L/T"), of_unit("m/s"));
        assert_eq!(dim("(L*L)^-1 * T"), of_unit("s/m2"));
        assert_eq!(dim("math::pow(L, 3)"), of_unit("m3"));
        assert_eq!(dim("math::sqrt(L*L)"), of_unit("m"));
        assert_eq!(dim("math::sqrt(L)"), Dim::Unknown);
        assert_eq!(dim("L^x"), Dim::Unknown);
        assert_eq!(dim("2*pi"), Dim::Number);
    }

    #[test]
    fn a_bare_number_adapts_and_mixed_sums_are_flagged() {
        assert_eq!(dim("K + 273.15"), of_unit("K"));
        assert_eq!(infer("K - 273.15", &dims).mixed, None);
        let mixed = infer("L + T", &dims);
        assert_eq!(mixed.dim, Dim::Unknown);
        assert!(mixed.mixed.is_some());
        assert!(infer("max(L, T)", &dims).mixed.is_some());
        assert_eq!(dim("math::hypot(L, L)"), of_unit("m"));
    }

    #[test]
    fn functions_want_dimensionless_arguments() {
        assert_eq!(infer("math::ln(L/L)", &dims).argument, None);
        let ln = infer("math::ln(K)", &dims);
        assert_eq!(ln.dim, of_unit(""));
        assert_eq!(
            ln.argument.map(|(what, _)| what),
            Some("el argumento de ln".into())
        );
        assert_eq!(infer("math::atan2(L, L)", &dims).dim, of_unit(""));
        assert!(infer("math::log(10, L)", &dims).argument.is_some());
        assert!(infer("math::log(L/L, 10)", &dims).argument.is_none());
    }

    #[test]
    fn exponents_are_walked_even_when_the_power_cannot_be_inferred() {
        let sum = infer("2^(L + T)", &dims);
        assert_eq!(sum.dim, Dim::Number);
        assert!(sum.mixed.is_some());
        assert!(infer("math::pow(2, math::ln(L))", &dims).argument.is_some());
        assert!(infer("L^(math::ln(K))", &dims).argument.is_some());
        let exponent = infer("2^L", &dims).argument;
        assert_eq!(exponent.map(|(what, _)| what), Some("el exponente".into()));
        assert!(infer("math::pow(10, T)", &dims).argument.is_some());
        assert!(infer("2^(L/L)", &dims).argument.is_none());
    }

    #[test]
    fn fractions_as_exponents_and_extreme_exponents() {
        assert_eq!(dim("(L*L)^(1/2)"), of_unit("m"));
        assert_eq!(dim("math::pow(L*L*L, 1/3)"), of_unit("m"));
        assert_eq!(dim("1/(T^-100*T^-28)"), Dim::Unknown);
        assert_eq!(dim("L^(0/0)"), Dim::Unknown);
    }

    #[test]
    fn radians_do_not_count_and_unknown_units_do_not_guess() {
        assert_eq!(dim("2*pi*f"), dim("w"));
        assert_eq!(infer("math::sin(phi)", &dims).argument, None);
        assert_eq!(dim("a/L"), Dim::Unknown);
        assert_eq!(dim("zzz"), Dim::Unknown);
        assert_eq!(dim("L >"), Dim::Unknown);
    }

    #[test]
    fn text_names_the_base_units() {
        assert_eq!(text([-1, 1, -1, 0, 0, 0, 0]), "kg·m⁻¹·s⁻¹");
        assert_eq!(text(NONE), "adimensional");
    }
}
