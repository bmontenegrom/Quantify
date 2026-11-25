type Dims = [i8; 8];

const NONE: Dims = [0; 8];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    pub factor: f64,
    pub offset: f64,
    pub dims: Dims,
}

struct Base {
    symbol: &'static str,
    factor: f64,
    dims: Dims,
    prefixable: bool,
}

const fn base(symbol: &'static str, factor: f64, dims: Dims, prefixable: bool) -> Base {
    Base {
        symbol,
        factor,
        dims,
        prefixable,
    }
}

const BASES: &[Base] = &[
    base("m", 1.0, [1, 0, 0, 0, 0, 0, 0, 0], true),
    base("%", 1e-2, NONE, false),
    base("ppm", 1e-6, NONE, false),
    base("ppb", 1e-9, NONE, false),
    base("g", 1e-3, [0, 1, 0, 0, 0, 0, 0, 0], true),
    base("s", 1.0, [0, 0, 1, 0, 0, 0, 0, 0], true),
    base("A", 1.0, [0, 0, 0, 1, 0, 0, 0, 0], true),
    base("K", 1.0, [0, 0, 0, 0, 1, 0, 0, 0], true),
    base("mol", 1.0, [0, 0, 0, 0, 0, 1, 0, 0], true),
    base("cd", 1.0, [0, 0, 0, 0, 0, 0, 1, 0], true),
    base("L", 1e-3, [3, 0, 0, 0, 0, 0, 0, 0], true),
    base("l", 1e-3, [3, 0, 0, 0, 0, 0, 0, 0], true),
    base("Hz", 1.0, [0, 0, -1, 0, 0, 0, 0, 0], true),
    base("N", 1.0, [1, 1, -2, 0, 0, 0, 0, 0], true),
    base("Pa", 1.0, [-1, 1, -2, 0, 0, 0, 0, 0], true),
    base("J", 1.0, [2, 1, -2, 0, 0, 0, 0, 0], true),
    base("W", 1.0, [2, 1, -3, 0, 0, 0, 0, 0], true),
    base("C", 1.0, [0, 0, 1, 1, 0, 0, 0, 0], true),
    base("V", 1.0, [2, 1, -3, -1, 0, 0, 0, 0], true),
    base("ohm", 1.0, [2, 1, -3, -2, 0, 0, 0, 0], true),
    base("Ω", 1.0, [2, 1, -3, -2, 0, 0, 0, 0], true),
    base("F", 1.0, [-2, -1, 4, 2, 0, 0, 0, 0], true),
    base("H", 1.0, [2, 1, -2, -2, 0, 0, 0, 0], true),
    base("M", 1e3, [-3, 0, 0, 0, 0, 1, 0, 0], true),
    base("rad", 1.0, [0, 0, 0, 0, 0, 0, 0, 1], true),
    base("min", 60.0, [0, 0, 1, 0, 0, 0, 0, 0], false),
    base("h", 3600.0, [0, 0, 1, 0, 0, 0, 0, 0], false),
    base("hs", 3600.0, [0, 0, 1, 0, 0, 0, 0, 0], false),
    base("°C", 1.0, [0, 0, 0, 0, 1, 0, 0, 0], false),
    base(
        "°",
        std::f64::consts::PI / 180.0,
        [0, 0, 0, 0, 0, 0, 0, 1],
        false,
    ),
];

const PREFIXES: &[(&str, i32, f64)] = &[
    ("p", -12, 1e-12),
    ("n", -9, 1e-9),
    ("µ", -6, 1e-6),
    ("u", -6, 1e-6),
    ("m", -3, 1e-3),
    ("c", -2, 1e-2),
    ("d", -1, 1e-1),
    ("h", 2, 1e2),
    ("k", 3, 1e3),
    ("M", 6, 1e6),
    ("G", 9, 1e9),
    ("T", 12, 1e12),
];

const SEPARATORS: [char; 5] = ['/', '.', '·', '*', ' '];

static INPUT_PREFIXES: std::sync::LazyLock<Vec<(&str, i32)>> = std::sync::LazyLock::new(|| {
    let mut prefixes: Vec<(&str, i32)> = PREFIXES
        .iter()
        .map(|&(prefix, power, _)| (prefix, power))
        .filter(|(prefix, power)| *prefix != "u" && power % 3 == 0)
        .chain([("", 0)])
        .collect();
    prefixes.sort_by_key(|(_, power)| *power);
    prefixes
});

const CELSIUS_OFFSET: f64 = 273.15;

const WHOLE: &[(&str, Unit)] = &[(
    "% m/v",
    Unit {
        factor: 10.0,
        offset: 0.0,
        dims: [-3, 1, 0, 0, 0, 0, 0, 0],
    },
)];

const CANDIDATES: &[&str] = &[
    "nm", "µm", "mm", "cm", "m", "km", "µm2", "mm2", "cm2", "m2", "km2", "µL", "mL", "L", "cm3",
    "m3", "µg", "mg", "g", "kg", "µs", "ms", "s", "min", "h", "°C", "K", "µmol", "mmol", "mol",
    "µM", "mM", "M", "mmol/L", "mol/L", "mg/L", "g/L", "g/cm3", "g/mL", "kg/m3", "µV", "mV", "V",
    "kV", "µA", "mA", "A", "ohm", "kohm", "Mohm", "pF", "nF", "µF", "mF", "F", "µH", "mH", "H",
    "Hz", "kHz", "MHz", "Pa", "hPa", "kPa", "MPa", "°", "rad", "°/s", "rad/s", "%", "ppm", "ppb",
];

static PARSED_CANDIDATES: std::sync::LazyLock<Vec<(&str, Unit)>> = std::sync::LazyLock::new(|| {
    CANDIDATES
        .iter()
        .filter_map(|c| Some((*c, parse(c)?)))
        .collect()
});

impl Unit {
    /// Dos unidades son compatibles si miden lo mismo (mismas dimensiones).
    ///
    /// ```
    /// use quantify::units::parse;
    /// assert!(parse("g/cm3").unwrap().compatible(&parse("kg/m3").unwrap()));
    /// assert!(!parse("s").unwrap().compatible(&parse("g").unwrap()));
    /// ```
    pub fn compatible(&self, other: &Unit) -> bool {
        self.dims == other.dims
    }

    fn times(self, other: Unit, exp: i32) -> Option<Unit> {
        let exp_dims = i8::try_from(exp).ok()?;
        let mut dims = self.dims;
        for (d, o) in dims.iter_mut().zip(other.dims) {
            *d = d.checked_add(o.checked_mul(exp_dims)?)?;
        }
        let factor = self.factor * other.factor.powi(exp);
        (factor.is_normal()).then_some(Unit {
            factor,
            offset: 0.0,
            dims,
        })
    }
}

const ONE: Unit = Unit {
    factor: 1.0,
    offset: 0.0,
    dims: NONE,
};

fn lookup(symbol: &str) -> Option<(Unit, bool)> {
    BASES.iter().find(|b| b.symbol == symbol).map(|b| {
        (
            Unit {
                factor: b.factor,
                offset: 0.0,
                dims: b.dims,
            },
            b.prefixable,
        )
    })
}

fn split_prefix(symbol: &str) -> Option<(&'static str, i32, f64, Unit)> {
    PREFIXES.iter().find_map(|&(prefix, power, factor)| {
        match lookup(symbol.strip_prefix(prefix)?) {
            Some((unit, true)) => Some((prefix, power, factor, unit)),
            _ => None,
        }
    })
}

fn symbol_unit(symbol: &str) -> Option<Unit> {
    if let Some((unit, _)) = lookup(symbol) {
        return Some(unit);
    }
    split_prefix(symbol).map(|(_, _, factor, unit)| Unit {
        factor: unit.factor * factor,
        ..unit
    })
}

fn factor_unit(token: &str) -> Option<(Unit, i32, &str)> {
    if token == "1" {
        return Some((ONE, 1, token));
    }
    let digits = token.len() - token.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (mut symbol, exponent) = token.split_at(token.len() - digits);
    let mut exp: i32 = if exponent.is_empty() {
        1
    } else {
        exponent.parse().ok()?
    };
    if !exponent.is_empty() {
        if let Some(rest) = symbol.strip_suffix('-') {
            symbol = rest;
            exp = -exp;
        }
        symbol = symbol.strip_suffix('^').unwrap_or(symbol);
    }
    if exp == 0 || symbol == "C" {
        return None;
    }
    Some((symbol_unit(symbol)?, exp, symbol))
}

fn product(text: &str) -> Option<Unit> {
    let mut factors = 0;
    let mut celsius = false;
    let unit = text.split(['.', '·', '*']).try_fold(ONE, |acc, piece| {
        let mut tokens = piece.split_whitespace().peekable();
        tokens.peek()?;
        tokens.try_fold(acc, |acc, token| {
            let (unit, exp, symbol) = factor_unit(token)?;
            if token != "1" {
                factors += 1;
                celsius = exp == 1 && symbol == "°C";
            }
            acc.times(unit, exp)
        })
    })?;
    Some(Unit {
        offset: if factors == 1 && celsius {
            CELSIUS_OFFSET
        } else {
            0.0
        },
        ..unit
    })
}

/// Interpreta una unidad como `m/s2`, `Pa.s` o `°C`: la vacía es adimensional y la desconocida, `None`.
///
/// ```
/// use quantify::units::parse;
/// assert_eq!(parse("cm").unwrap().factor, 0.01);
/// assert!((parse("g/cm3").unwrap().factor - 1000.0).abs() < 1e-9);
/// assert_eq!(parse("°C").unwrap().offset, 273.15);
/// assert_eq!(parse("").unwrap().factor, 1.0);
/// assert!(parse("div").is_none());
/// ```
pub fn parse(text: &str) -> Option<Unit> {
    let normalized = normalize(text);
    let text = normalized.trim();
    if NOT_UNITS.contains(&text) {
        return None;
    }
    if text.is_empty() {
        return Some(ONE);
    }
    if let Some((_, unit)) = WHOLE.iter().find(|(symbol, _)| *symbol == text) {
        return Some(*unit);
    }
    let (numerator, denominator) = match text.split_once('/') {
        Some((n, d)) => {
            let d = d.trim();
            let d = d
                .strip_prefix('(')
                .and_then(|d| d.strip_suffix(')'))
                .unwrap_or(d);
            if d.contains('/') || d.trim().is_empty() || n.trim().is_empty() {
                return None;
            }
            (n, Some(d))
        }
        None => (text, None),
    };
    let unit = product(numerator)?;
    match denominator {
        Some(d) => match product(d)? {
            d if d.factor == 1.0 && d.dims == NONE => Some(unit),
            d => unit.times(d, -1),
        },
        None => Some(unit),
    }
}

const NOT_UNITS: &[&str] = &["pH", "% m/m", "% v/v"];

/// Si la unidad es conocida: se interpreta, o se sabe a propósito que no se convierte (pH, % m/m).
///
/// ```
/// use quantify::units::known;
/// assert!(known("cm") && known("") && known("pH") && known("% m/m"));
/// assert!(!known("div"));
/// ```
pub fn known(text: &str) -> bool {
    NOT_UNITS.contains(&normalize(text).trim()) || parse(text).is_some()
}

fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'º' => '°',
            'μ' => 'µ',
            'Ω' => 'Ω',
            '²' => '2',
            '³' => '3',
            '¹' => '1',
            '⁰' => '0',
            '⁴' => '4',
            '⁵' => '5',
            '⁶' => '6',
            '⁷' => '7',
            '⁸' => '8',
            '⁹' => '9',
            '⁻' | '−' => '-',
            '⋅' | '∙' | '•' => '·',
            '×' => '*',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("° C", "°C")
        .replace("%m/v", "% m/v")
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InputUnit {
    pub label: String,
    pub factor: f64,
    pub offset: f64,
}

fn affine(from: Unit, to: Unit) -> (f64, f64) {
    (
        from.factor / to.factor,
        (from.offset - to.offset) / to.factor,
    )
}

fn relative(label: &str, unit: Unit, declared: Unit) -> InputUnit {
    let (factor, offset) = affine(unit, declared);
    InputUnit {
        label: label.to_string(),
        factor,
        offset,
    }
}

/// Unidades para cargar un valor en `declared`, la declarada primero: valor = `escrito·factor + offset`.
///
/// ```
/// use quantify::units::input_units;
/// let litros = input_units("L");
/// assert_eq!(litros[0].label, "L");
/// let ml = litros.iter().find(|u| u.label == "mL").unwrap();
/// assert!((ml.factor - 1e-3).abs() < 1e-15);
/// let kelvin = input_units("°C").into_iter().find(|u| u.label == "K").unwrap();
/// assert!((kelvin.offset + 273.15).abs() < 1e-9);
/// assert_eq!(input_units("div").len(), 1);
/// assert_eq!(input_units("").len(), 1);
/// ```
pub fn input_units(declared: &str) -> Vec<InputUnit> {
    let mut out = vec![InputUnit {
        label: declared.trim().to_string(),
        factor: 1.0,
        offset: 0.0,
    }];
    let normalized = normalize(declared);
    let declared = normalized.as_str();
    let Some(unit) = parse(declared).filter(|_| !declared.is_empty()) else {
        return out;
    };
    for (candidate, c) in PARSED_CANDIDATES
        .iter()
        .filter(|(_, c)| c.compatible(&unit))
    {
        push_new(&mut out, relative(candidate, *c, unit));
    }
    if let Some((base, exponent, own)) = unprefixed(declared).filter(|_| out.len() == 1) {
        let compound = declared.contains(SEPARATORS);
        let reasonable = |power: i32| {
            let away = power - own;
            if compound {
                away.abs() <= 3
            } else {
                (away * exponent).abs() <= 12
            }
        };
        for (prefix, _) in INPUT_PREFIXES.iter().filter(|(_, p)| reasonable(*p)) {
            let label = format!("{prefix}{base}");
            if let Some(c) = parse(&label).filter(|c| c.compatible(&unit)) {
                push_new(&mut out, relative(&label, c, unit));
            }
        }
    }
    out
}

fn push_new(out: &mut Vec<InputUnit>, input: InputUnit) {
    let same_factor = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs());
    let same_offset = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-12);
    if !out
        .iter()
        .any(|u| same_factor(u.factor, input.factor) && same_offset(u.offset, input.offset))
    {
        out.push(input);
    }
}

fn unprefixed(declared: &str) -> Option<(&str, i32, i32)> {
    let first = declared.split(SEPARATORS).next()?;
    let (_, exponent, symbol) = factor_unit(first)?;
    match lookup(symbol) {
        Some((_, true)) => Some((declared, exponent, 0)),
        Some((_, false)) => None,
        None => split_prefix(symbol)
            .map(|(prefix, power, _, _)| (&declared[prefix.len()..], exponent, power)),
    }
}

#[derive(Debug, PartialEq)]
pub enum Mismatch {
    Incompatible,
    Unknown(String),
}

/// Pasa de `from` a `to` como `valor·factor + offset`: vacía o igual no convierte.
///
/// ```
/// use quantify::units::{conversion, Mismatch};
/// assert_eq!(conversion("mm", "cm").map(|(f, _)| (f * 10.0).round()), Ok(1.0));
/// assert_eq!(conversion("°C", "K"), Ok((1.0, 273.15)));
/// assert_eq!(conversion("", "%"), Ok((1.0, 0.0)));
/// assert_eq!(conversion("s", "g"), Err(Mismatch::Incompatible));
/// assert_eq!(conversion("div", "mm"), Err(Mismatch::Unknown("div".into())));
/// ```
pub fn conversion(from: &str, to: &str) -> Result<(f64, f64), Mismatch> {
    let (normal_from, normal_to) = (normalize(from), normalize(to));
    if normal_from.trim().is_empty() || normal_to.trim().is_empty() || normal_from == normal_to {
        return Ok((1.0, 0.0));
    }
    match (parse(from), parse(to)) {
        (Some(f), Some(t)) if f.compatible(&t) => Ok(affine(f, t)),
        (Some(_), Some(_)) => Err(Mismatch::Incompatible),
        (None, _) => Err(Mismatch::Unknown(from.trim().to_string())),
        (_, None) => Err(Mismatch::Unknown(to.trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1e-300)
    }

    fn si(text: &str) -> Unit {
        parse(text).unwrap_or_else(|| panic!("no reconoce {text:?}"))
    }

    #[test]
    fn units_in_use_have_their_si_factor_and_dimensions() {
        let cases: &[(&str, f64, Dims)] = &[
            ("V", 1.0, [2, 1, -3, -1, 0, 0, 0, 0]),
            ("ohm", 1.0, [2, 1, -3, -2, 0, 0, 0, 0]),
            ("kohm", 1e3, [2, 1, -3, -2, 0, 0, 0, 0]),
            ("cm", 1e-2, [1, 0, 0, 0, 0, 0, 0, 0]),
            ("mm", 1e-3, [1, 0, 0, 0, 0, 0, 0, 0]),
            ("mm2", 1e-6, [2, 0, 0, 0, 0, 0, 0, 0]),
            ("g", 1e-3, [0, 1, 0, 0, 0, 0, 0, 0]),
            ("kg", 1.0, [0, 1, 0, 0, 0, 0, 0, 0]),
            ("Hz", 1.0, [0, 0, -1, 0, 0, 0, 0, 0]),
            ("1/s", 1.0, [0, 0, -1, 0, 0, 0, 0, 0]),
            ("°", std::f64::consts::PI / 180.0, [0, 0, 0, 0, 0, 0, 0, 1]),
            ("rad/s", 1.0, [0, 0, -1, 0, 0, 0, 0, 1]),
            ("m/s2", 1.0, [1, 0, -2, 0, 0, 0, 0, 0]),
            ("kg/m3", 1.0, [-3, 1, 0, 0, 0, 0, 0, 0]),
            ("g/cm3", 1e3, [-3, 1, 0, 0, 0, 0, 0, 0]),
            ("Pa.s", 1.0, [-1, 1, -1, 0, 0, 0, 0, 0]),
            ("m3/s", 1.0, [3, 0, -1, 0, 0, 0, 0, 0]),
            ("N/m", 1.0, [0, 1, -2, 0, 0, 0, 0, 0]),
            ("W", 1.0, [2, 1, -3, 0, 0, 0, 0, 0]),
            ("uF", 1e-6, [-2, -1, 4, 2, 0, 0, 0, 0]),
            ("nF", 1e-9, [-2, -1, 4, 2, 0, 0, 0, 0]),
            ("mV", 1e-3, [2, 1, -3, -1, 0, 0, 0, 0]),
            ("H", 1.0, [2, 1, -2, -2, 0, 0, 0, 0]),
            ("m^-1", 1.0, [-1, 0, 0, 0, 0, 0, 0, 0]),
            ("s-1", 1.0, [0, 0, -1, 0, 0, 0, 0, 0]),
            ("J/(mol.K)", 1.0, [2, 1, -2, 0, -1, -1, 0, 0]),
            ("mol/L", 1e3, [-3, 0, 0, 0, 0, 1, 0, 0]),
            ("% m/v", 10.0, [-3, 1, 0, 0, 0, 0, 0, 0]),
        ];
        for (text, factor, d) in cases {
            let u = si(text);
            assert!(close(u.factor, *factor), "{text}: {} != {factor}", u.factor);
            assert_eq!(u.dims, *d, "{text}");
        }
    }

    #[test]
    fn whole_symbols_win_over_prefix_plus_base() {
        assert!(close(si("M").factor, 1e3));
        assert_eq!(si("M").dims, si("mol/L").dims);
        assert!(close(si("mM").factor, 1.0));
        assert!(close(si("MPa").factor, 1e6));
        assert!(close(si("min").factor, 60.0));
        assert!(close(si("h").factor, 3600.0));
        assert!(close(si("hPa").factor, 100.0));
        assert_eq!(si("cd").dims, [0, 0, 0, 0, 0, 0, 1, 0]);
        assert_eq!(si("mC").dims, [0, 0, 1, 1, 0, 0, 0, 0]);
        assert!(parse("kmin").is_none());
        assert!(parse("k°").is_none());
    }

    #[test]
    fn unknown_or_malformed_units_are_not_recognized() {
        for text in [
            "div", "m/s/s", "/s", "m/", "xyz", "m3x", "m256", "J64", "m127.m", "km120", "*", ".",
            "· ", "m/(.)", "*/s", "m/()", "pH", "m0", "s-0", "% m/m", "% v/v", "C", "C/min",
            "J/(g.C)", "C/s", "m.", "*s", "m..s", "m* /s",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
    }

    #[test]
    fn spacing_variants_are_the_same_unit() {
        assert_eq!(si("° C"), si("°C"));
        assert_eq!(si("º  C"), si("°C"));
        assert_eq!(si("J / (mol K)"), si("J/(mol.K)"));
        assert_eq!(si("%m/v"), si("% m/v"));
        assert_eq!(si("%  m/v"), si("% m/v"));
        let labels: Vec<String> = input_units("m²/s").into_iter().map(|u| u.label).collect();
        assert_eq!(labels[0], "m²/s");
        assert!(labels.contains(&"mm2/s".to_string()));
    }

    #[test]
    fn every_prefix_factor_matches_its_power() {
        for (prefix, power, factor) in PREFIXES {
            assert!(close(*factor, 10f64.powi(*power)), "{prefix}");
        }
    }

    #[test]
    fn hs_is_hours_not_hectoseconds() {
        assert!(close(si("hs").factor, 3600.0));
        let min = input_units("hs")
            .into_iter()
            .find(|u| u.label == "min")
            .unwrap();
        assert!(close(min.factor, 1.0 / 60.0));
        let squared: Vec<String> = input_units("hs2").into_iter().map(|u| u.label).collect();
        assert!(!squared.iter().any(|l| l == "ks2"), "{squared:?}");
    }

    #[test]
    fn lookalike_characters_are_normalized() {
        assert_eq!(si("ºC"), si("°C"));
        assert_eq!(si("\u{3bc}m"), si("\u{b5}m"));
        assert_eq!(si("m/s²"), si("m/s2"));
        assert_eq!(si("cm³"), si("cm3"));
        assert_eq!(si("s⁻¹"), si("s-1"));
        assert_eq!(si("Pa⋅s"), si("Pa.s"));
        assert_eq!(si("Pa∙s"), si("Pa.s"));
        assert_eq!(si("Pa•s"), si("Pa.s"));
        assert_eq!(si("s−1"), si("s-1"));
        assert_eq!(si("m⁴"), si("m4"));
        assert_eq!(si("s⁻⁹"), si("s-9"));
        assert_eq!(si("kg×m"), si("kg*m"));
        assert_eq!(si("k\u{2126}"), si("kohm"));
    }

    #[test]
    fn input_units_skip_synonyms_of_what_is_already_offered() {
        let labels = |unit: &str| -> Vec<String> {
            input_units(unit).into_iter().map(|u| u.label).collect()
        };
        assert!(!labels("uF").contains(&"µF".to_string()));
        assert!(!labels("M").contains(&"mol/L".to_string()));
        assert!(!labels("mM").contains(&"mmol/L".to_string()));
        assert!(!labels("k\u{2126}").contains(&"kohm".to_string()));
        assert!(labels("kg/m3").contains(&"g/cm3".to_string()));
        assert!(!labels("kg/m3").contains(&"g/mL".to_string()));
        assert!(labels("Pa").contains(&"MPa".to_string()));
        assert_eq!(labels("m2"), ["m2", "µm2", "mm2", "cm2", "km2"]);
        assert!(labels("mm2").contains(&"µm2".to_string()));
        assert!(labels("cm2").contains(&"km2".to_string()));
        let volume = labels("m3");
        assert!(volume.contains(&"L".to_string()) && !volume.contains(&"Mm3".to_string()));
        let quartic = labels("m4");
        assert!(quartic.contains(&"km4".to_string()), "{quartic:?}");
        assert!(quartic.contains(&"mm4".to_string()), "{quartic:?}");
        assert!(!quartic.contains(&"Mm4".to_string()), "{quartic:?}");
        assert!(!quartic.contains(&"µm4".to_string()), "{quartic:?}");
        let force = labels("kg.m/s2");
        assert!(force.contains(&"g.m/s2".to_string()), "{force:?}");
        assert!(!force.contains(&"mg.m/s2".to_string()), "{force:?}");
        let slow = labels("µm/s");
        assert!(slow.contains(&"nm/s".to_string()), "{slow:?}");
        assert!(!slow.contains(&"km/s".to_string()), "{slow:?}");
        let tiny = labels("N");
        assert!(tiny.contains(&"pN".to_string()), "{tiny:?}");
        assert!(tiny.contains(&"nN".to_string()), "{tiny:?}");
        let angular = labels("rad/s");
        assert!(angular.contains(&"°/s".to_string()), "{angular:?}");
        assert!(!angular.contains(&"Grad/s".to_string()), "{angular:?}");
    }

    #[test]
    fn temperature_keeps_its_offset_only_alone() {
        assert_eq!(si("°C").offset, 273.15);
        assert_eq!(si("J/°C").offset, 0.0);
        for same in [
            "°C1", "°C^1", "°C01", "°C^01", "1 °C", "1.°C", "°C/1", "°C/(1)",
        ] {
            assert_eq!(si(same), si("°C"), "{same}");
        }
        assert_eq!(si("°C2").offset, 0.0);
        assert_eq!(si("1/°C").offset, 0.0);
        assert!(si("°C").compatible(&si("K")));
    }

    #[test]
    fn input_units_offer_compatible_units_with_their_conversion() {
        let labels = |unit: &str| -> Vec<String> {
            input_units(unit).into_iter().map(|u| u.label).collect()
        };
        assert_eq!(labels("cm")[0], "cm");
        assert!(labels("cm").contains(&"mm".to_string()));
        assert!(!labels("cm").contains(&"g".to_string()));
        assert!(labels("mol/L").contains(&"mM".to_string()));
        assert!(labels("kg/m3").contains(&"g/cm3".to_string()));
        let g_cm3 = input_units("kg/m3")
            .into_iter()
            .find(|u| u.label == "g/cm3")
            .unwrap();
        assert!(close(g_cm3.factor, 1000.0));
        assert_eq!(labels("N/m"), ["N/m", "mN/m", "kN/m"]);
        assert!(labels("Pa.s").contains(&"mPa.s".to_string()));
        assert_eq!(labels("mPa.s")[0], "mPa.s");
        assert!(labels("mPa.s").contains(&"Pa.s".to_string()));
        assert!(labels("mm2").contains(&"cm2".to_string()));
        assert!(!labels("mm2")[1..].contains(&"mm2".to_string()));
        let pa_s = input_units("mPa.s")
            .into_iter()
            .find(|u| u.label == "Pa.s")
            .unwrap();
        assert!(close(pa_s.factor, 1e3));
        assert_eq!(labels("m/s2"), ["m/s2", "mm/s2", "km/s2"]);
        assert_eq!(labels("div"), ["div"]);
        assert_eq!(labels("*"), ["*"]);
        assert_eq!(labels(""), [""]);
        let kelvin = input_units("°C")
            .into_iter()
            .find(|u| u.label == "K")
            .unwrap();
        assert!(close(300.0 * kelvin.factor + kelvin.offset, 300.0 - 273.15));
    }

    #[test]
    fn conversion_converts_between_compatible_units() {
        let factor = |from: &str, to: &str| conversion(from, to).map(|(f, _)| f);
        assert!(close(factor("mm", "m").unwrap(), 1e-3));
        assert!(close(factor("g/cm3", "kg/m3").unwrap(), 1e3));
        assert!(close(factor("M", "mol/L").unwrap(), 1.0));
        assert_eq!(factor("s", "g"), Err(Mismatch::Incompatible));
        assert_eq!(factor("div", "div"), Ok(1.0));
        assert_eq!(factor("", ""), Ok(1.0));
        assert_eq!(
            conversion(" di  v ", "mm"),
            Err(Mismatch::Unknown("di  v".into()))
        );
        assert!(close(si("%/°C").factor, 1e-2));
        assert!(si("ppm/K").compatible(&si("1/K")));
        assert!(close(si("%/min").factor, 1e-2 / 60.0));
    }

    #[test]
    fn every_unit_of_the_library_is_recognized() {
        fn units(value: &serde_json::Value, out: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, v) in map {
                        match (key.as_str(), v) {
                            ("unit", serde_json::Value::String(s)) => out.push(s.clone()),
                            _ => units(v, out),
                        }
                    }
                }
                serde_json::Value::Array(items) => items.iter().for_each(|v| units(v, out)),
                _ => {}
            }
        }
        let mut found = vec![];
        for (_, json) in crate::practices::LIBRARY {
            units(&serde_json::from_str(json).unwrap(), &mut found);
        }
        assert!(found.len() > 50);
        for unit in found.iter().filter(|u| *u != "div") {
            assert!(parse(unit).is_some(), "no reconoce {unit:?}");
        }
    }

    #[test]
    fn every_constant_of_the_catalog_is_usable() {
        let catalog: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../static/constants.json")).unwrap();
        assert!(catalog.len() >= 18);
        for c in &catalog {
            let unit = c["unit"].as_str().unwrap();
            assert!(parse(unit).is_some(), "no reconoce {unit:?}");
            if let Some(q) = c["quantity"].as_str() {
                assert!(
                    crate::magnitudes::MAGNITUDES
                        .iter()
                        .any(|(name, _)| *name == q),
                    "{q:?} no está en el catálogo de magnitudes"
                );
            }
            assert!(c["value"].as_f64().is_some_and(f64::is_finite), "{c}");
            assert!(
                c["u"].is_null() || c["u"].as_f64().is_some_and(|u| u >= 0.0),
                "{c}"
            );
            assert!(!c["source"].as_str().unwrap().trim().is_empty(), "{c}");
        }
        let value = |symbol: &str| {
            catalog
                .iter()
                .find(|c| c["symbol"] == symbol)
                .and_then(|c| c["value"].as_f64())
                .unwrap()
        };
        let exact = |derived: f64, product: f64| ((derived - product) / product).abs() < 1e-15;
        assert!(exact(value("R"), value("N_A") * value("k_B")));
        assert!(exact(value("F"), value("N_A") * value("q_e")));
    }

    #[tokio::test]
    async fn every_unit_of_the_seeded_instruments_is_recognized() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        crate::db::seed_academic(&pool).await.unwrap();
        crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
            .await
            .unwrap();
        let units: Vec<String> = sqlx::query_scalar(
            "SELECT unit FROM instruments UNION SELECT unit FROM instrument_scales",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(units.len() >= 5, "{units:?}");
        for unit in units.iter().filter(|u| *u != "div") {
            assert!(parse(unit).is_some(), "no reconoce {unit:?}");
        }
    }
}
