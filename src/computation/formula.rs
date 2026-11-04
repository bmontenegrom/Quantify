use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Node, Value};
use std::collections::HashMap;

pub(crate) const CONSTANTS: [(&str, f64); 2] =
    [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

pub(crate) fn compile_formula(formula: &str, allowed: &[String]) -> anyhow::Result<Node> {
    let tree = build_operator_tree(formula)
        .map_err(|err| anyhow::anyhow!("la formula \"{formula}\" no es valida: {err}"))?;
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

/// Valida que una fórmula compile y solo use símbolos de `allowed` (o las constantes `pi` y `e`).
///
/// ```
/// use quantify::computation::check_formula;
/// let allowed = vec!["L".to_string(), "T".to_string()];
/// assert!(check_formula("4*pi^2*L/T^2", &allowed).is_ok());
/// let err = check_formula("L * g", &allowed).unwrap_err().to_string();
/// assert!(err.contains("\"g\""));
/// ```
pub fn check_formula(formula: &str, allowed: &[String]) -> anyhow::Result<()> {
    compile_formula(formula, allowed).map(|_| ())
}

pub(crate) fn formula_symbols(formula: &str) -> Vec<String> {
    build_operator_tree(formula)
        .map(|tree| tree.iter_variable_identifiers().map(String::from).collect())
        .unwrap_or_default()
}

pub(crate) fn eval_compiled(tree: &Node, values: &HashMap<&str, f64>) -> f64 {
    let mut context = HashMapContext::new();
    for (name, value) in CONSTANTS {
        let _ = context.set_value(name.to_string(), Value::Float(value));
    }
    for (symbol, value) in values {
        if context
            .set_value((*symbol).to_string(), Value::Float(*value))
            .is_err()
        {
            return f64::NAN;
        }
    }
    tree.eval_float_with_context(&context).unwrap_or(f64::NAN)
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
