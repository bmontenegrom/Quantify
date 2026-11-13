use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Node, Value};
use std::collections::HashMap;

pub(crate) const CONSTANTS: [(&str, f64); 2] =
    [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

fn float_literals(formula: &str) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        let exponent = i >= 3
            && matches!(chars[i - 1], '+' | '-')
            && matches!(chars[i - 2], 'e' | 'E')
            && (chars[i - 3].is_ascii_digit() || chars[i - 3] == '.');
        let starts_number = chars[i].is_ascii_digit()
            && !exponent
            && (i == 0 || !(chars[i - 1].is_alphanumeric() || matches!(chars[i - 1], '_' | '.')));
        if !starts_number {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        while i < chars.len() && chars[i].is_ascii_digit() {
            out.push(chars[i]);
            i += 1;
        }
        let next = chars.get(i).copied();
        if !next.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.')) {
            out.push_str(".0");
        }
    }
    out
}

fn parse(formula: &str) -> evalexpr::EvalexprResult<Node> {
    build_operator_tree(&float_literals(formula))
}

pub(crate) fn compile_formula(formula: &str, allowed: &[String]) -> anyhow::Result<Node> {
    let tree = parse(formula)
        .map_err(|err| anyhow::anyhow!("la formula \"{formula}\" no es valida: {err}"))?;
    if let Some(name) = tree
        .iter_function_identifiers()
        .find(|name| !FUNCTIONS.iter().any(|(known, _)| known == name))
    {
        anyhow::bail!(
            "la fórmula \"{formula}\" usa la función \"{name}\", que no está entre las disponibles"
        );
    }
    let mut unknown: Vec<&str> = Vec::new();
    for var in tree.iter_variable_identifiers() {
        let is_constant = CONSTANTS.iter().any(|(name, _)| *name == var);
        if !is_constant && !allowed.iter().any(|s| s == var) && !unknown.contains(&var) {
            unknown.push(var);
        }
    }
    match unknown.as_slice() {
        [] => Ok(tree),
        [var] => anyhow::bail!(
            "la formula \"{formula}\" usa el simbolo \"{var}\", que no es una magnitud de la practica"
        ),
        vars => anyhow::bail!(
            "la formula \"{formula}\" usa los simbolos \"{}\", que no son magnitudes de la practica",
            vars.join("\", \"")
        ),
    }
}

/// Funciones que se aceptan en una fórmula, con su descripción. Quedan afuera las escalonadas
/// (`floor`, `round`, `ceil`), porque la incertidumbre se propaga con una derivada numérica.
///
/// ```
/// use quantify::computation::FUNCTIONS;
/// assert!(FUNCTIONS.iter().any(|(name, _)| *name == "math::sqrt"));
/// assert!(!FUNCTIONS.iter().any(|(name, _)| *name == "random"));
/// assert!(!FUNCTIONS.iter().any(|(name, _)| *name == "round"));
/// ```
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("math::sqrt", "raíz cuadrada"),
    ("math::ln", "logaritmo natural"),
    ("math::log10", "logaritmo en base 10"),
    ("math::exp", "exponencial"),
    ("math::abs", "valor absoluto"),
    ("math::sin", "seno (radianes)"),
    ("math::cos", "coseno (radianes)"),
    ("math::tan", "tangente (radianes)"),
    ("math::asin", "arcoseno (radianes)"),
    ("math::acos", "arcocoseno (radianes)"),
    ("math::atan", "arcotangente (radianes)"),
    ("math::atan2", "arcotangente de y, x (radianes)"),
    ("math::sinh", "seno hiperbólico"),
    ("math::cosh", "coseno hiperbólico"),
    ("math::tanh", "tangente hiperbólica"),
    ("math::asinh", "seno hiperbólico inverso"),
    ("math::acosh", "coseno hiperbólico inverso"),
    ("math::atanh", "tangente hiperbólica inversa"),
    ("math::pow", "potencia: base, exponente"),
    ("math::hypot", "hipotenusa: raíz de x² + y²"),
    ("math::cbrt", "raíz cúbica"),
    ("math::log", "logaritmo: valor, base"),
    ("math::log2", "logaritmo en base 2"),
    ("math::exp2", "potencia de 2"),
    ("min", "mínimo"),
    ("max", "máximo"),
];

/// Valida que una fórmula compile, solo use símbolos de `allowed` (o las constantes `pi` y `e`)
/// y solo llame funciones de [`FUNCTIONS`].
///
/// ```
/// use quantify::computation::check_formula;
/// let allowed = vec!["L".to_string(), "T".to_string()];
/// assert!(check_formula("4*pi^2*L/T^2", &allowed).is_ok());
/// assert!(check_formula("math::sqrt(L)", &allowed).is_ok());
/// let err = check_formula("L * g", &allowed).unwrap_err().to_string();
/// assert!(err.contains("\"g\""));
/// let err = check_formula("math::foo(L)", &allowed).unwrap_err().to_string();
/// assert!(err.contains("\"math::foo\""));
/// let err = check_formula("math::pow(L)", &allowed).unwrap_err().to_string();
/// assert!(err.contains("cantidad de argumentos"));
/// let err = check_formula("L > 1", &allowed).unwrap_err().to_string();
/// assert!(err.contains("no da un número"));
/// assert!(check_formula("4*pi*1e-7*L", &allowed).is_ok());
/// ```
pub fn check_formula(formula: &str, allowed: &[String]) -> anyhow::Result<()> {
    let tree = compile_formula(formula, allowed)?;
    let ones: HashMap<&str, f64> = allowed.iter().map(|s| (s.as_str(), 1.0)).collect();
    match evaluate(&tree, &ones) {
        Ok(_) => Ok(()),
        Err(Unusable::NotANumber) => anyhow::bail!(
            "la fórmula \"{formula}\" no da un número (¿quedó una comparación o una coma de más?)"
        ),
        Err(Unusable::Failed) if tree.iter_function_identifiers().next().is_some() => {
            anyhow::bail!(
                "la fórmula \"{formula}\" no se puede calcular: revisá que cada función tenga la cantidad de argumentos que pide"
            )
        }
        Err(Unusable::Failed) => {
            anyhow::bail!("la fórmula \"{formula}\" no se puede calcular")
        }
    }
}

enum Unusable {
    NotANumber,
    Failed,
}

pub(crate) fn formula_symbols(formula: &str) -> Vec<String> {
    parse(formula)
        .map(|tree| tree.iter_variable_identifiers().map(String::from).collect())
        .unwrap_or_default()
}

fn evaluate(tree: &Node, values: &HashMap<&str, f64>) -> Result<f64, Unusable> {
    let mut context = HashMapContext::new();
    for (name, value) in CONSTANTS {
        let _ = context.set_value(name.to_string(), Value::Float(value));
    }
    for (symbol, value) in values {
        context
            .set_value((*symbol).to_string(), Value::Float(*value))
            .map_err(|_| Unusable::Failed)?;
    }
    match tree.eval_with_context(&context) {
        Ok(Value::Float(value)) => Ok(value),
        Ok(Value::Int(value)) => Ok(value as f64),
        Ok(_) => Err(Unusable::NotANumber),
        Err(_) => Err(Unusable::Failed),
    }
}

pub(crate) fn eval_compiled(tree: &Node, values: &HashMap<&str, f64>) -> f64 {
    evaluate(tree, values).unwrap_or(f64::NAN)
}

pub(crate) fn chain_order(uses: &[Vec<usize>]) -> (Vec<usize>, Vec<usize>) {
    let mut ordered: Vec<usize> = Vec::with_capacity(uses.len());
    let mut done = vec![false; uses.len()];
    loop {
        let ready: Vec<usize> = (0..uses.len())
            .filter(|&i| !done[i] && uses[i].iter().all(|&d| done[d]))
            .collect();
        if ready.is_empty() {
            break;
        }
        for i in ready {
            done[i] = true;
            ordered.push(i);
        }
    }
    let blocked = (0..uses.len()).filter(|&i| !done[i]).collect();
    (ordered, blocked)
}
