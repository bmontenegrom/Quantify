use evalexpr::{
    build_operator_tree, ContextWithMutableVariables, HashMapContext, Node, Operator, Value,
};
use std::collections::HashMap;

pub(crate) const CONSTANTS: [(&str, f64); 2] =
    [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

fn parse(formula: &str) -> evalexpr::EvalexprResult<Node> {
    let mut tree = build_operator_tree(formula)?;
    for operator in tree.iter_operators_mut() {
        if let Operator::Const { value } = operator {
            if let Value::Int(int) = *value {
                *value = Value::Float(int as f64);
            }
        }
    }
    Ok(tree)
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

/// Funciones que se aceptan en una fórmula, con su descripción.
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

/// Valida que una fórmula compile, use solo símbolos de `allowed` y funciones de [`FUNCTIONS`].
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
/// let err = check_formula("math::sqrt(L) * \"x\"", &allowed).unwrap_err().to_string();
/// assert!(!err.contains("cantidad de argumentos"));
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
        Err(Unusable::Failed(
            evalexpr::EvalexprError::WrongFunctionArgumentAmount { .. }
            | evalexpr::EvalexprError::ExpectedTuple { .. }
            | evalexpr::EvalexprError::ExpectedFixedLengthTuple { .. }
            | evalexpr::EvalexprError::ExpectedRangedLengthTuple { .. },
        )) => {
            anyhow::bail!(
                "la fórmula \"{formula}\" no se puede calcular: revisá que cada función tenga la cantidad de argumentos que pide y que sea una sola expresión"
            )
        }
        Err(Unusable::Failed(_)) => {
            anyhow::bail!("la fórmula \"{formula}\" no se puede calcular")
        }
    }
}

enum Unusable {
    NotANumber,
    Failed(evalexpr::EvalexprError),
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
        let _ = context.set_value((*symbol).to_string(), Value::Float(*value));
    }
    match tree.eval_with_context(&context) {
        Ok(Value::Float(value)) => Ok(value),
        Ok(_) => Err(Unusable::NotANumber),
        Err(err) => Err(Unusable::Failed(err)),
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
