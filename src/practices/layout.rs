use super::{PartInput, PracticePart, PracticeSection, SectionInput};
use sqlx::SqlitePool;
use uuid::Uuid;

const LEGACY_LAYOUT: &str = "2026-11-partes-y-secciones";

pub async fn parts_for(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Vec<PracticePart>> {
    Ok(sqlx::query_as::<_, PracticePart>(
        "SELECT id, practice_id, title, position, analysis_kind FROM practice_parts \
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
    for (table, is_quantity) in [("practice_quantities", true), ("practice_results", false)] {
        let members: Vec<(String, String)> = sqlx::query_as(&format!(
            "SELECT section_id, symbol FROM {table} \
             WHERE practice_id = ?1 AND section_id IS NOT NULL ORDER BY position, symbol"
        ))
        .bind(practice_id)
        .fetch_all(pool)
        .await?;
        for (section_id, symbol) in members {
            if let Some(section) = sections.iter_mut().find(|s| s.id == section_id) {
                if is_quantity {
                    section.quantities.push(symbol);
                } else {
                    section.results.push(symbol);
                }
            }
        }
    }
    Ok(sections)
}

pub async fn create_part(
    pool: &SqlitePool,
    practice_id: &str,
    input: &PartInput,
) -> anyhow::Result<PracticePart> {
    let id = Uuid::new_v4().to_string();
    let position =
        crate::db::next_position(pool, "practice_parts", "practice_id", practice_id).await?;
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
        analysis_kind: None,
        effective_kind: "estadistico".into(),
    })
}

pub async fn save_part_kinds(
    pool: &SqlitePool,
    definition: &super::PracticeDefinition,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    write_kinds(&mut tx, definition).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_part_saving_kinds(
    pool: &SqlitePool,
    part_id: &str,
    after: &super::PracticeDefinition,
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let deleted = sqlx::query("DELETE FROM practice_parts WHERE id = ?1 AND practice_id = ?2")
        .bind(part_id)
        .bind(&after.practice_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Ok(false);
    }
    write_kinds(&mut tx, after).await?;
    tx.commit().await?;
    Ok(true)
}

async fn write_kinds(
    conn: &mut sqlx::SqliteConnection,
    definition: &super::PracticeDefinition,
) -> anyhow::Result<()> {
    for part in &definition.parts {
        sqlx::query(
            "UPDATE practice_parts SET analysis_kind = ?1 WHERE id = ?2 AND practice_id = ?3",
        )
        .bind(&part.analysis_kind)
        .bind(&part.id)
        .bind(&definition.practice_id)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query("UPDATE practices SET analysis_kind = ?1 WHERE id = ?2")
        .bind(&definition.analysis_kind)
        .bind(&definition.practice_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
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

async fn belongs(
    pool: &SqlitePool,
    table: &str,
    id: &str,
    practice_id: &str,
) -> anyhow::Result<bool> {
    let found: Option<(i64,)> = sqlx::query_as(&format!(
        "SELECT 1 FROM {table} WHERE id = ?1 AND practice_id = ?2"
    ))
    .bind(id)
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

async fn part_belongs(
    pool: &SqlitePool,
    practice_id: &str,
    part_id: Option<&str>,
) -> anyhow::Result<bool> {
    match part_id {
        Some(part_id) => belongs(pool, "practice_parts", part_id, practice_id).await,
        None => Ok(true),
    }
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
    let position =
        crate::db::next_position(pool, "practice_sections", "practice_id", practice_id).await?;
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
        if !belongs(pool, "practice_sections", section_id, practice_id).await? {
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

async fn apply_layout(
    conn: &mut sqlx::SqliteConnection,
    layout: &LegacyLayout,
) -> anyhow::Result<()> {
    let practice_id = layout.practice_id;
    for (position, (key, title)) in layout.parts.iter().enumerate() {
        sqlx::query(
            "INSERT INTO practice_parts (id, practice_id, title, position) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(format!("{practice_id}-{key}"))
        .bind(practice_id)
        .bind(title)
        .bind(position as i64)
        .execute(&mut *conn)
        .await?;
    }
    for (position, sec) in layout.sections.iter().enumerate() {
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
        .execute(&mut *conn)
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
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    Ok(())
}

async fn apply_layout_if_empty(
    conn: &mut sqlx::SqliteConnection,
    layout: &LegacyLayout,
) -> anyhow::Result<()> {
    let empty: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM practices p WHERE p.id = ?1 \
         AND NOT EXISTS (SELECT 1 FROM practice_parts WHERE practice_id = ?1) \
         AND NOT EXISTS (SELECT 1 FROM practice_sections WHERE practice_id = ?1)",
    )
    .bind(layout.practice_id)
    .fetch_optional(&mut *conn)
    .await?;
    if empty.is_some() {
        apply_layout(conn, layout).await?;
    }
    Ok(())
}

pub(super) async fn apply_p3_layout(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<()> {
    let layout = LegacyLayout {
        practice_id: "p3-relajacion",
        parts: vec![
            ("directa", "Parte 1: Relajación directa"),
            ("desfasaje", "Parte 2: Desfasaje"),
        ],
        sections: vec![
            section(
                Some("directa"),
                "Circuito RC y semidescarga",
                &["R", "Rint", "C", "T_oc", "tmedio"],
                &["tau_teorico", "tau_exp"],
            ),
            section(
                Some("desfasaje"),
                "Elipse por frecuencia",
                &["f", "a", "b"],
                &["tau"],
            ),
        ],
    };
    apply_layout_if_empty(conn, &layout).await
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
    for layout in legacy_layouts() {
        apply_layout_if_empty(&mut tx, &layout).await?;
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_LAYOUT)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

const SHARED_RESULTS_TO_PARTS: &str = "2026-11-mensurandos-compartidos-a-su-parte";

pub(super) async fn migrate_shared_results_to_parts(pool: &SqlitePool) -> anyhow::Result<()> {
    let (done,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
        .bind(SHARED_RESULTS_TO_PARTS)
        .fetch_one(pool)
        .await?;
    if done > 0 {
        return Ok(());
    }
    let practice_ids: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT practice_id FROM practice_parts")
            .fetch_all(pool)
            .await?;
    let mut moves: Vec<(String, Option<String>)> = Vec::new();
    for practice_id in practice_ids {
        let Some(def) = super::definition(pool, &practice_id).await? else {
            continue;
        };
        for result in &def.results {
            if def.result_part(&result.symbol).is_some() {
                continue;
            }
            let Some(part) = def.inferred_part(&result.formula) else {
                continue;
            };
            let in_part = || {
                def.sections
                    .iter()
                    .filter(|s| s.part_id.as_deref() == Some(part))
            };
            let section = in_part()
                .find(|s| !s.results.is_empty())
                .or_else(|| in_part().next())
                .map(|s| s.id.clone());
            moves.push((result.id.clone(), section));
        }
    }
    let mut tx = pool.begin().await?;
    for (result_id, section_id) in moves {
        sqlx::query("UPDATE practice_results SET section_id = ?1 WHERE id = ?2")
            .bind(section_id)
            .bind(result_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(SHARED_RESULTS_TO_PARTS)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
