use super::{PartInput, PracticePart, PracticeSection, SectionInput};
use sqlx::SqlitePool;
use uuid::Uuid;

const LEGACY_LAYOUT: &str = "2026-11-partes-y-secciones";

pub async fn parts_for(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Vec<PracticePart>> {
    Ok(sqlx::query_as::<_, PracticePart>(
        "SELECT id, practice_id, title, position FROM practice_parts \
         WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

pub async fn sections_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeSection>> {
    let mut sections = sqlx::query_as::<_, PracticeSection>(
        "SELECT id, practice_id, part_id, title, position FROM practice_sections \
         WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?;
    for section in &mut sections {
        section.quantities = sqlx::query_scalar(
            "SELECT symbol FROM practice_quantities WHERE section_id = ?1 ORDER BY position, symbol",
        )
        .bind(&section.id)
        .fetch_all(pool)
        .await?;
        section.results = sqlx::query_scalar(
            "SELECT symbol FROM practice_results WHERE section_id = ?1 ORDER BY position, symbol",
        )
        .bind(&section.id)
        .fetch_all(pool)
        .await?;
    }
    Ok(sections)
}

async fn next_position(pool: &SqlitePool, table: &str, practice_id: &str) -> anyhow::Result<i64> {
    let (next,): (i64,) = sqlx::query_as(&format!(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM {table} WHERE practice_id = ?1"
    ))
    .bind(practice_id)
    .fetch_one(pool)
    .await?;
    Ok(next)
}

pub async fn create_part(
    pool: &SqlitePool,
    practice_id: &str,
    input: &PartInput,
) -> anyhow::Result<PracticePart> {
    let id = Uuid::new_v4().to_string();
    let position = next_position(pool, "practice_parts", practice_id).await?;
    sqlx::query(
        "INSERT INTO practice_parts (id, practice_id, title, position) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.title.trim())
    .bind(position)
    .execute(pool)
    .await?;
    Ok(PracticePart {
        id,
        practice_id: practice_id.to_string(),
        title: input.title.trim().to_string(),
        position,
    })
}

pub async fn update_part(
    pool: &SqlitePool,
    practice_id: &str,
    part_id: &str,
    input: &PartInput,
) -> anyhow::Result<bool> {
    let changed =
        sqlx::query("UPDATE practice_parts SET title = ?3 WHERE id = ?2 AND practice_id = ?1")
            .bind(practice_id)
            .bind(part_id)
            .bind(input.title.trim())
            .execute(pool)
            .await?
            .rows_affected();
    Ok(changed > 0)
}

pub async fn delete_part(
    pool: &SqlitePool,
    practice_id: &str,
    part_id: &str,
) -> anyhow::Result<bool> {
    let deleted = sqlx::query("DELETE FROM practice_parts WHERE id = ?2 AND practice_id = ?1")
        .bind(practice_id)
        .bind(part_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(deleted > 0)
}

async fn part_belongs(
    pool: &SqlitePool,
    practice_id: &str,
    part_id: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(part_id) = part_id else {
        return Ok(true);
    };
    let found: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM practice_parts WHERE id = ?1 AND practice_id = ?2")
            .bind(part_id)
            .bind(practice_id)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

pub async fn create_section(
    pool: &SqlitePool,
    practice_id: &str,
    input: &SectionInput,
) -> anyhow::Result<Option<PracticeSection>> {
    let part_id = input.part_id.as_deref().filter(|p| !p.is_empty());
    if !part_belongs(pool, practice_id, part_id).await? {
        return Ok(None);
    }
    let id = Uuid::new_v4().to_string();
    let position = next_position(pool, "practice_sections", practice_id).await?;
    sqlx::query(
        "INSERT INTO practice_sections (id, practice_id, part_id, title, position) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(part_id)
    .bind(input.title.trim())
    .bind(position)
    .execute(pool)
    .await?;
    Ok(Some(PracticeSection {
        id,
        practice_id: practice_id.to_string(),
        part_id: part_id.map(str::to_string),
        title: input.title.trim().to_string(),
        position,
        quantities: vec![],
        results: vec![],
    }))
}

pub async fn update_section(
    pool: &SqlitePool,
    practice_id: &str,
    section_id: &str,
    input: &SectionInput,
) -> anyhow::Result<Option<bool>> {
    let part_id = input.part_id.as_deref().filter(|p| !p.is_empty());
    if !part_belongs(pool, practice_id, part_id).await? {
        return Ok(None);
    }
    let changed = sqlx::query(
        "UPDATE practice_sections SET title = ?3, part_id = ?4 WHERE id = ?2 AND practice_id = ?1",
    )
    .bind(practice_id)
    .bind(section_id)
    .bind(input.title.trim())
    .bind(part_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(Some(changed > 0))
}

pub async fn delete_section(
    pool: &SqlitePool,
    practice_id: &str,
    section_id: &str,
) -> anyhow::Result<bool> {
    let deleted = sqlx::query("DELETE FROM practice_sections WHERE id = ?2 AND practice_id = ?1")
        .bind(practice_id)
        .bind(section_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(deleted > 0)
}

#[derive(Debug, Clone, Copy)]
pub enum SectionMember {
    Quantity,
    Result,
}

pub async fn set_member_section(
    pool: &SqlitePool,
    practice_id: &str,
    member: SectionMember,
    member_id: &str,
    section_id: Option<&str>,
) -> anyhow::Result<bool> {
    if let Some(section_id) = section_id {
        let found: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM practice_sections WHERE id = ?1 AND practice_id = ?2")
                .bind(section_id)
                .bind(practice_id)
                .fetch_optional(pool)
                .await?;
        if found.is_none() {
            return Ok(false);
        }
    }
    let table = match member {
        SectionMember::Quantity => "practice_quantities",
        SectionMember::Result => "practice_results",
    };
    let changed = sqlx::query(&format!(
        "UPDATE {table} SET section_id = ?3 WHERE id = ?2 AND practice_id = ?1"
    ))
    .bind(practice_id)
    .bind(member_id)
    .bind(section_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(changed > 0)
}

struct LegacySection {
    part: Option<&'static str>,
    title: String,
    quantities: Vec<String>,
    results: Vec<String>,
}

fn section(
    part: Option<&'static str>,
    title: &str,
    quantities: &[&str],
    results: &[&str],
) -> LegacySection {
    LegacySection {
        part,
        title: title.to_string(),
        quantities: quantities.iter().map(|s| s.to_string()).collect(),
        results: results.iter().map(|s| s.to_string()).collect(),
    }
}

struct LegacyLayout {
    practice_id: &'static str,
    parts: Vec<(&'static str, &'static str)>,
    sections: Vec<LegacySection>,
}

fn legacy_layouts() -> Vec<LegacyLayout> {
    let ranuras: Vec<String> = (1..=9).map(|i| format!("b{i}")).collect();
    let mut brazos: Vec<&str> = vec!["m_goma", "rho_f", "b_E"];
    brazos.extend(ranuras.iter().map(String::as_str));
    let mut hidro = vec![
        section(None, "Datos comunes", &["g_ac"], &[]),
        section(
            Some("arq"),
            "Goma, fluido y brazos de la balanza",
            &brazos,
            &[],
        ),
    ];
    for k in 1..=3 {
        let masses: Vec<String> = (1..=9).map(|i| format!("m{k}_{i}")).collect();
        let masses: Vec<&str> = masses.iter().map(String::as_str).collect();
        let results: &[&str] = if k == 3 {
            &["E_medio", "rho_medio"]
        } else {
            &[]
        };
        hidro.push(section(
            Some("arq"),
            &format!("Medida {k} — masas por ranura (la ranura sin pesa queda en 0)"),
            &masses,
            results,
        ));
    }
    hidro.push(section(Some("ts"), "Marco de Wilhelmy", &["l", "d"], &[]));
    for k in 1..=3 {
        let weights = [format!("mw{k}"), format!("dw{k}")];
        let weights: Vec<&str> = weights.iter().map(String::as_str).collect();
        let results: &[&str] = if k == 3 { &["gamma_medio"] } else { &[] };
        hidro.push(section(
            Some("ts"),
            &format!("Medida {k} — pesa que sustituye a la pelicula"),
            &weights,
            results,
        ));
    }
    vec![
        LegacyLayout {
            practice_id: "p1-estadistica",
            parts: vec![
                ("op1", "Operador 1"),
                ("op2", "Operador 2"),
                ("op3", "Operador 3"),
            ],
            sections: vec![
                section(Some("op1"), "Operador 1 — Períodos", &["T1"], &["g1"]),
                section(
                    Some("op2"),
                    "Operador 2 — Períodos (opcional)",
                    &["T2"],
                    &["g2"],
                ),
                section(
                    Some("op3"),
                    "Operador 3 — Períodos (opcional)",
                    &["T3"],
                    &["g3"],
                ),
                section(None, "Medidas", &["L", "t_med"], &["gamma", "Q"]),
            ],
        },
        LegacyLayout {
            practice_id: "p2-cc",
            parts: vec![
                ("serie", "Serie"),
                ("paralelo", "Paralelo"),
                ("potencia", "Curva de potencia"),
            ],
            sections: vec![
                section(
                    None,
                    "Resistencias (medidas una vez, valen para las tres partes)",
                    &["R1", "R2", "R3"],
                    &[],
                ),
                section(
                    Some("serie"),
                    "Circuito serie",
                    &["Vg_s", "RA_s", "VR1_s", "VR2_s", "VR3_s"],
                    &["I_s", "VR1_s_t", "VR2_s_t", "VR3_s_t"],
                ),
                section(
                    Some("paralelo"),
                    "Circuito paralelo",
                    &["Vg_p", "RA_p", "VR1_p", "VR2_p", "VR3_p"],
                    &["I_p", "VR1_p_t", "VR2_p_t", "VR3_p_t"],
                ),
                section(
                    Some("potencia"),
                    "Curva de potencia — datos",
                    &["Vg_c", "RA_c", "R", "I"],
                    &["RP_max_t", "P_max_t", "P_max_e", "RP_max_e"],
                ),
            ],
        },
        LegacyLayout {
            practice_id: "hidrostatica",
            parts: vec![
                ("arq", "Parte 1: Arquimedes"),
                ("ts", "Parte 2: Tension superficial"),
            ],
            sections: hidro,
        },
    ]
}

pub(super) async fn migrate_legacy_layout(pool: &SqlitePool) -> anyhow::Result<()> {
    let (done,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
        .bind(LEGACY_LAYOUT)
        .fetch_one(pool)
        .await?;
    if done > 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for LegacyLayout {
        practice_id,
        parts,
        sections,
    } in legacy_layouts()
    {
        let exists: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM practices p WHERE p.id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM practice_parts WHERE practice_id = ?1) \
             AND NOT EXISTS (SELECT 1 FROM practice_sections WHERE practice_id = ?1)",
        )
        .bind(practice_id)
        .fetch_optional(&mut *tx)
        .await?;
        if exists.is_none() {
            continue;
        }
        for (position, (key, title)) in parts.iter().enumerate() {
            sqlx::query(
                "INSERT INTO practice_parts (id, practice_id, title, position) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(format!("{practice_id}-{key}"))
            .bind(practice_id)
            .bind(title)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
        }
        for (position, sec) in sections.iter().enumerate() {
            let section_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO practice_sections (id, practice_id, part_id, title, position) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(&section_id)
            .bind(practice_id)
            .bind(sec.part.map(|key| format!("{practice_id}-{key}")))
            .bind(&sec.title)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
            for (table, symbols) in [
                ("practice_quantities", &sec.quantities),
                ("practice_results", &sec.results),
            ] {
                for symbol in symbols {
                    sqlx::query(&format!(
                        "UPDATE {table} SET section_id = ?1 WHERE practice_id = ?2 AND symbol = ?3"
                    ))
                    .bind(&section_id)
                    .bind(practice_id)
                    .bind(symbol)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_LAYOUT)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
