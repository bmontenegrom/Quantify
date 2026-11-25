pub const MAGNITUDES: &[(&str, &[&str])] = &[
    ("longitud", &["distancia", "largo"]),
    ("masa", &[]),
    ("tiempo", &[]),
    (
        "voltaje",
        &[
            "tension",
            "tension electrica",
            "diferencia de potencial",
            "ddp",
        ],
    ),
    ("corriente", &["corriente electrica", "intensidad"]),
    ("resistencia", &["resistencia electrica"]),
    ("capacitancia", &["capacidad"]),
    ("inductancia", &[]),
    ("frecuencia", &[]),
    ("densidad", &[]),
    ("temperatura", &[]),
    ("aceleracion", &[]),
    ("viscosidad", &[]),
    ("volumen", &[]),
    ("adimensional", &[]),
    ("cantidad de sustancia", &["moles"]),
    ("concentracion", &["molaridad"]),
    ("absorbancia", &[]),
    ("masa molar", &[]),
];

/// El nombre canónico de una magnitud física: minúsculas, sin tildes y con los sinónimos unificados.
///
/// ```
/// use quantify::magnitudes::canonical;
/// assert_eq!(canonical(" Tensión "), "voltaje");
/// assert_eq!(canonical("Diferencia  de potencial"), "voltaje");
/// assert_eq!(canonical("Aceleración"), "aceleracion");
/// assert_eq!(canonical("Caudal"), "caudal");
/// assert_eq!(canonical(""), "");
/// ```
pub fn canonical(text: &str) -> String {
    let plain: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect();
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    MAGNITUDES
        .iter()
        .find(|(name, aliases)| *name == plain || aliases.contains(&plain.as_str()))
        .map_or(plain, |(name, _)| (*name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_and_alias_is_already_canonical_text() {
        for (name, aliases) in MAGNITUDES {
            assert_eq!(canonical(name), *name);
            for alias in *aliases {
                assert_eq!(canonical(alias), *name, "{alias}");
            }
        }
    }
}
