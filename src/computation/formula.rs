//! Compilación y evaluación de fórmulas de usuario (texto) con `evalexpr`.

use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Node, Value};
use std::collections::HashMap;

/// Constantes disponibles en cualquier fórmula (además de las funciones `math::*` de evalexpr).
pub(crate) const CONSTANTS: [(&str, f64); 2] =
    [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

/// Compila una fórmula y valida que todas sus variables sean símbolos declarados (los de
/// `allowed`) o constantes conocidas (`pi`, `e`). Devuelve el árbol precompilado.
pub(crate) fn compile_formula(formula: &str, allowed: &[String]) -> anyhow::Result<Node> {
    let tree = build_operator_tree(formula)
        .map_err(|err| anyhow::anyhow!("la formula \"{formula}\" no es valida: {err}"))?;
    for var in tree.iter_variable_identifiers() {
        let is_constant = CONSTANTS.iter().any(|(name, _)| *name == var);
        if !is_constant && !allowed.iter().any(|s| s == var) {
            anyhow::bail!(
                "la formula \"{formula}\" usa el simbolo \"{var}\", que no es una magnitud de la practica"
            );
        }
    }
    Ok(tree)
}

/// Valida que `formula` sea sintácticamente correcta y solo use símbolos de `allowed` (o las
/// constantes `pi`/`e`). Para validar fórmulas en el alta (p. ej. una magnitud intermedia) sin
/// esperar a que falle en el cálculo. Devuelve el error amigable de [`compile_formula`].
pub fn check_formula(formula: &str, allowed: &[String]) -> anyhow::Result<()> {
    compile_formula(formula, allowed).map(|_| ())
}

/// Evalúa una fórmula precompilada con los valores dados por símbolo (más las constantes
/// `pi`/`e`). Devuelve `NaN` si la evaluación falla, para no romper la propagación numérica.
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
