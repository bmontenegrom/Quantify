use super::PracticeTemplate;

macro_rules! library {
    ($($key:literal),* $(,)?) => {
        [$(($key, include_str!(concat!("../../templates/practices/", $key, ".json")))),*]
    };
}

pub const LIBRARY: [(&str, &str); 9] = library!(
    "p1-estadistica",
    "p2-cc",
    "p3-relajacion",
    "fluidos-1",
    "viscosidad",
    "fluidos-2",
    "filtros",
    "ca-rlc",
    "hidrostatica",
);

/// La plantilla del repo con esa clave, o `None` si no existe.
///
/// ```
/// let t = quantify::practices::library_template("fluidos-1").unwrap();
/// assert_eq!(t.schema_version, quantify::practices::TEMPLATE_VERSION);
/// ```
pub fn library_template(key: &str) -> Option<PracticeTemplate> {
    let (_, json) = LIBRARY.iter().find(|(k, _)| *k == key)?;
    Some(serde_json::from_str(json).expect("las plantillas del repo son JSON válido"))
}
