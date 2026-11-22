use super::PracticeTemplate;

macro_rules! library {
    ($($key:literal),* $(,)?) => {
        [$(($key, include_str!(concat!("../../templates/practices/", $key, ".json")))),*]
    };
}

pub const LIBRARY: &[(&str, &str)] = &library!(
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

/// La plantilla del repo con esa clave (`None` si no existe; `Some(Err)` si no se puede leer).
///
/// ```
/// let t = quantify::practices::library_template("fluidos-1").unwrap().unwrap();
/// assert_eq!(t.schema_version, quantify::practices::TEMPLATE_VERSION);
/// ```
pub fn library_template(key: &str) -> Option<anyhow::Result<PracticeTemplate>> {
    let (_, json) = LIBRARY.iter().find(|(k, _)| *k == key)?;
    Some(
        serde_json::from_str(json)
            .map_err(|e| anyhow::anyhow!("la plantilla {key} no se puede leer: {e}")),
    )
}
