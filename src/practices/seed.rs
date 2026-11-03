use super::crud::{
    create_aggregate, create_curve, create_intermediate, create_point_result, insert_quantity,
    insert_result,
};
use super::{
    AggregateInput, CurveInput, IntermediateInput, PointResultInput, QuantityInput, ResultInput,
};
use sqlx::{SqliteConnection, SqlitePool};

fn qty(symbol: &str, name: &str, unit: &str, repeated: bool, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated,
        quantity: Some(quantity.into()),
        is_given: false,
        replicas_per_point: None,
        per_point: true,
        has_uncertainty: true,
        optional: false,
        default_value: None,
        slot_count: None,
    }
}

fn qty_given(symbol: &str, name: &str, unit: &str, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated: false,
        quantity: Some(quantity.into()),
        is_given: true,
        replicas_per_point: None,
        per_point: false,
        has_uncertainty: true,
        optional: false,
        default_value: None,
        slot_count: None,
    }
}

fn no_u(mut q: QuantityInput) -> QuantityInput {
    q.has_uncertainty = false;
    q
}

fn with_default(mut q: QuantityInput, value: f64) -> QuantityInput {
    q.default_value = Some(Some(value));
    q
}

fn slots(mut q: QuantityInput, n: i64) -> QuantityInput {
    q.slot_count = Some(n);
    q
}

fn opt(mut q: QuantityInput) -> QuantityInput {
    q.optional = true;
    q
}

fn qty_replicas(
    symbol: &str,
    name: &str,
    unit: &str,
    quantity: &str,
    replicas: i64,
) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated: true,
        quantity: Some(quantity.into()),
        is_given: false,
        replicas_per_point: Some(replicas),
        per_point: true,
        has_uncertainty: true,
        optional: false,
        default_value: None,
        slot_count: None,
    }
}

fn qty_shared(symbol: &str, name: &str, unit: &str, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated: false,
        quantity: Some(quantity.into()),
        is_given: false,
        replicas_per_point: None,
        per_point: false,
        has_uncertainty: true,
        optional: false,
        default_value: None,
        slot_count: None,
    }
}

fn res(symbol: &str, name: &str, unit: &str, formula: &str) -> ResultInput {
    ResultInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        formula: formula.into(),
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
    }
}

fn res_final(symbol: &str, name: &str, unit: &str, formula: &str) -> ResultInput {
    ResultInput {
        is_final: true,
        ..res(symbol, name, unit, formula)
    }
}

fn res_no_u(mut r: ResultInput) -> ResultInput {
    r.has_uncertainty = false;
    r
}

async fn seed_practice(
    pool: &SqlitePool,
    practice_id: &str,
    quantities: &[QuantityInput],
    results: &[ResultInput],
) -> anyhow::Result<bool> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1")
            .bind(practice_id)
            .fetch_one(pool)
            .await?;
    if count.0 > 0 {
        return Ok(false);
    }
    let mut conn = pool.acquire().await?;
    for (pos, q) in quantities.iter().enumerate() {
        insert_quantity(&mut conn, practice_id, pos as i64 + 1, q).await?;
    }
    for (pos, r) in results.iter().enumerate() {
        insert_result(&mut conn, practice_id, pos as i64 + 1, r).await?;
    }
    Ok(true)
}

async fn wipe_practice_definition(
    conn: &mut SqliteConnection,
    practice_id: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "DELETE FROM submission_measurements WHERE quantity_id IN \
         (SELECT id FROM practice_quantities WHERE practice_id = ?1)",
    )
    .bind(practice_id)
    .execute(&mut *conn)
    .await?;
    for table in [
        "practice_point_results",
        "practice_aggregates",
        "practice_curves",
        "practice_intermediates",
        "practice_results",
        "practice_quantities",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE practice_id = ?1"))
            .bind(practice_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn quantity_missing(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1 AND symbol = ?2",
    )
    .bind(practice_id)
    .bind(symbol)
    .fetch_one(pool)
    .await?;
    Ok(count.0 == 0)
}

async fn result_missing(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_results WHERE practice_id = ?1 AND symbol = ?2",
    )
    .bind(practice_id)
    .bind(symbol)
    .fetch_one(pool)
    .await?;
    Ok(count.0 == 0)
}

async fn seed_p1_estadistica(pool: &SqlitePool) -> anyhow::Result<()> {
    seed_practice(
        pool,
        "p1-estadistica",
        &[
            qty_given("L", "Longitud del pendulo", "m", "longitud"),
            no_u(qty_given(
                "t_med",
                "Tiempo de semiamplitud (t1/2)",
                "s",
                "tiempo",
            )),
            qty("T1", "Periodo - Operador 1", "s", true, "tiempo"),
            opt(qty("T2", "Periodo - Operador 2", "s", true, "tiempo")),
            opt(qty("T3", "Periodo - Operador 3", "s", true, "tiempo")),
        ],
        &[
            res_no_u(res_final(
                "gamma",
                "Coeficiente de amortiguamiento",
                "1/s",
                "2*math::ln(2)/t_med",
            )),
            res_no_u(res_final(
                "Q",
                "Factor de calidad (usa el periodo del Operador 1)",
                "",
                "pi*t_med/(T1*math::ln(2))",
            )),
            res_final(
                "g1",
                "Aceleracion de gravedad - Operador 1",
                "m/s2",
                "4*pi^2*L/T1^2",
            ),
            res_final(
                "g2",
                "Aceleracion de gravedad - Operador 2",
                "m/s2",
                "4*pi^2*L/T2^2",
            ),
            res_final(
                "g3",
                "Aceleracion de gravedad - Operador 3",
                "m/s2",
                "4*pi^2*L/T3^2",
            ),
        ],
    )
    .await?;
    if quantity_missing(pool, "p1-estadistica", "T1").await? {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM submission_measurements WHERE quantity_id IN \
             (SELECT id FROM practice_quantities WHERE practice_id = 'p1-estadistica' AND symbol = 'T')",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM practice_quantities WHERE practice_id = 'p1-estadistica' AND symbol = 'T'",
        )
        .execute(&mut *tx)
        .await?;
        let base_pos: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(position), 0) FROM practice_quantities WHERE practice_id = ?1",
        )
        .bind("p1-estadistica")
        .fetch_one(&mut *tx)
        .await?;
        for (i, q) in [
            qty("T1", "Periodo - Operador 1", "s", true, "tiempo"),
            opt(qty("T2", "Periodo - Operador 2", "s", true, "tiempo")),
            opt(qty("T3", "Periodo - Operador 3", "s", true, "tiempo")),
        ]
        .iter()
        .enumerate()
        {
            insert_quantity(&mut tx, "p1-estadistica", base_pos.0 + i as i64 + 1, q).await?;
        }
        sqlx::query(
            "UPDATE practice_quantities SET has_uncertainty = 0 \
             WHERE practice_id = 'p1-estadistica' AND symbol = 't_med'",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE practice_quantities SET optional = 1 \
             WHERE practice_id = 'p1-estadistica' AND symbol IN ('T2', 'T3')",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    if result_missing(pool, "p1-estadistica", "g1").await? {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM practice_results WHERE practice_id = 'p1-estadistica' \
             AND symbol IN ('g', 'Tmedio', 'delta')",
        )
        .execute(&mut *tx)
        .await?;
        let base_pos: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(position), 0) FROM practice_results WHERE practice_id = ?1",
        )
        .bind("p1-estadistica")
        .fetch_one(&mut *tx)
        .await?;
        for (i, r) in [
            res_final(
                "g1",
                "Aceleracion de gravedad - Operador 1",
                "m/s2",
                "4*pi^2*L/T1^2",
            ),
            res_final(
                "g2",
                "Aceleracion de gravedad - Operador 2",
                "m/s2",
                "4*pi^2*L/T2^2",
            ),
            res_final(
                "g3",
                "Aceleracion de gravedad - Operador 3",
                "m/s2",
                "4*pi^2*L/T3^2",
            ),
        ]
        .iter()
        .enumerate()
        {
            insert_result(&mut tx, "p1-estadistica", base_pos.0 + i as i64 + 1, r).await?;
        }
        sqlx::query(
            "UPDATE practice_results SET has_uncertainty = 0 \
             WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q')",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE practice_results SET formula = 'pi*t_med/(T1*math::ln(2))', \
                 name = 'Factor de calidad (usa el periodo del Operador 1)' \
             WHERE practice_id = 'p1-estadistica' AND symbol = 'Q'",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    sqlx::query(
        "UPDATE practice_results SET is_final = 1 \
         WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE practices SET operator_count = NULL \
         WHERE id = 'p1-estadistica' AND operator_count IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_p3_relajacion(pool: &SqlitePool) -> anyhow::Result<()> {
    let fresh = seed_practice(
        pool,
        "p3-relajacion",
        &[
            qty_shared("R", "Resistencia", "ohm", "resistencia"),
            qty_given(
                "Rint",
                "Resistencia interna de la fuente",
                "ohm",
                "resistencia",
            ),
            qty_shared("C", "Capacitancia", "F", "capacitancia"),
            qty_shared("T_oc", "Periodo de la onda cuadrada", "s", "tiempo"),
            qty_shared("tmedio", "Tiempo de semidescarga (t1/2)", "s", "tiempo"),
            qty("f", "Frecuencia", "Hz", true, "frecuencia"),
            qty(
                "a",
                "Amplitud de la senal en el eje y de la elipse",
                "div",
                true,
                "longitud",
            ),
            qty(
                "b",
                "Interseccion de la elipse con el eje y",
                "div",
                true,
                "longitud",
            ),
        ],
        &[
            res(
                "tau_teorico",
                "Tiempo de relajacion teorico",
                "s",
                "(R + Rint) * C",
            ),
            res_final(
                "tau_exp",
                "Tiempo de relajacion experimental",
                "s",
                "tmedio / math::ln(2)",
            ),
            res_final("tau", "Constante de tiempo RC (desfasaje)", "s", "slope"),
        ],
    )
    .await?;
    if fresh {
        super::layout::apply_p3_layout(pool).await?;
    }
    Ok(())
}

async fn seed_p2_cc(pool: &SqlitePool) -> anyhow::Result<()> {
    if !quantity_missing(pool, "p2-cc", "R").await?
        && quantity_missing(pool, "p2-cc", "Vg_s").await?
    {
        let mut tx = pool.begin().await?;
        wipe_practice_definition(&mut tx, "p2-cc").await?;
        tx.commit().await?;
    }
    let fresh_p2cc = seed_practice(
        pool,
        "p2-cc",
        &[
            qty_shared("R1", "Resistencia R1 (compartida)", "ohm", "resistencia"),
            qty_shared("R2", "Resistencia R2 (compartida)", "ohm", "resistencia"),
            qty_shared("R3", "Resistencia R3 (compartida)", "ohm", "resistencia"),
            qty_shared("Vg_s", "Voltaje de la fuente", "V", "voltaje"),
            qty_given(
                "RA_s",
                "Resistencia interna del amperimetro",
                "ohm",
                "resistencia",
            ),
            qty_shared("VR1_s", "Voltaje medido en R1", "V", "voltaje"),
            qty_shared("VR2_s", "Voltaje medido en R2", "V", "voltaje"),
            qty_shared("VR3_s", "Voltaje medido en R3", "V", "voltaje"),
            qty_shared("Vg_p", "Voltaje de la fuente", "V", "voltaje"),
            qty_given(
                "RA_p",
                "Resistencia interna del amperimetro",
                "ohm",
                "resistencia",
            ),
            qty_shared("VR1_p", "Voltaje medido en R1", "V", "voltaje"),
            qty_shared("VR2_p", "Voltaje medido en R2", "V", "voltaje"),
            qty_shared("VR3_p", "Voltaje medido en R3", "V", "voltaje"),
            qty_shared("Vg_c", "Voltaje de la fuente", "V", "voltaje"),
            qty_given(
                "RA_c",
                "Resistencia interna del amperimetro",
                "ohm",
                "resistencia",
            ),
            qty(
                "R",
                "Resistencia externa (carga variable)",
                "ohm",
                false,
                "resistencia",
            ),
            qty("I", "Corriente de carga", "A", false, "corriente"),
        ],
        &[
            res_final(
                "I_s",
                "Corriente teorica",
                "A",
                "Vg_s / (R1 + R2 + R3 + RA_s)",
            ),
            res_final(
                "VR1_s_t",
                "Voltaje teorico en R1",
                "V",
                "Vg_s * R1 / (R1 + R2 + R3 + RA_s)",
            ),
            res_final(
                "VR2_s_t",
                "Voltaje teorico en R2",
                "V",
                "Vg_s * R2 / (R1 + R2 + R3 + RA_s)",
            ),
            res_final(
                "VR3_s_t",
                "Voltaje teorico en R3",
                "V",
                "Vg_s * R3 / (R1 + R2 + R3 + RA_s)",
            ),
            res(
                "Req",
                "Resistencia equivalente",
                "ohm",
                "R1 + RA_p + R2*R3/(R2+R3)",
            ),
            res_final(
                "I_p",
                "Corriente teorica",
                "A",
                "Vg_p / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "VR1_p_t",
                "Voltaje teorico en R1",
                "V",
                "Vg_p * R1 / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "VR2_p_t",
                "Voltaje teorico en R2",
                "V",
                "Vg_p * (R2*R3/(R2+R3)) / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "VR3_p_t",
                "Voltaje teorico en R3",
                "V",
                "Vg_p * (R2*R3/(R2+R3)) / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "RP_max_t",
                "Resistencia de maxima transferencia teorica (Rth)",
                "ohm",
                "RA_c + R2*R3/(R2+R3)",
            ),
            res_final(
                "P_max_t",
                "Potencia maxima teorica",
                "W",
                "Vg_c*Vg_c/(4*(RA_c + R2*R3/(R2+R3)))",
            ),
            res_final(
                "P_max_e",
                "Potencia maxima experimental (de la tabla)",
                "W",
                "P_max",
            ),
            res_final(
                "RP_max_e",
                "Resistencia de maxima transferencia experimental",
                "ohm",
                "R_at_P_max",
            ),
        ],
    )
    .await?;
    if fresh_p2cc {
        create_intermediate(
            pool,
            "p2-cc",
            IntermediateInput {
                symbol: "P".into(),
                name: "Potencia disipada en R".into(),
                unit: "W".into(),
                formula: "I*I*R".into(),
            },
        )
        .await?;
        create_curve(
            pool,
            "p2-cc",
            CurveInput {
                x_formula: "R".into(),
                y_formula: "P".into(),
                x_log: false,
            },
        )
        .await?;
    }
    Ok(())
}

async fn seed_fluidos1(pool: &SqlitePool) -> anyhow::Result<()> {
    let fresh = seed_practice(
        pool,
        "fluidos-1",
        &[
            qty("h", "Altura del Mariotte", "m", false, "longitud"),
            qty_replicas("V", "Volumen recogido", "m3", "volumen", 2),
            qty_replicas("t", "Tiempo de descarga", "s", "tiempo", 2),
            qty_given("R", "Radio del capilar", "m", "longitud"),
            qty_given("L", "Longitud del capilar", "m", "longitud"),
            qty_given("g", "Aceleracion de la gravedad", "m/s2", "aceleracion"),
            qty_shared("rho", "Densidad del agua", "kg/m3", "densidad"),
            qty_shared(
                "Temp",
                "Temperatura del agua (referencia)",
                "C",
                "temperatura",
            ),
        ],
        &[res_final(
            "mu",
            "Viscosidad del agua",
            "Pa.s",
            "slope*(pi*rho*g*R^4)/(8*L)",
        )],
    )
    .await?;
    if fresh {
        create_intermediate(
            pool,
            "fluidos-1",
            IntermediateInput {
                symbol: "Q".into(),
                name: "Caudal medio".into(),
                unit: "m3/s".into(),
                formula: "V/t".into(),
            },
        )
        .await?;
        create_point_result(
            pool,
            "fluidos-1",
            PointResultInput {
                symbol: "Re".into(),
                name: "Numero de Reynolds".into(),
                unit: "".into(),
                formula: "2*rho*Q/(pi*mu*R)".into(),
            },
        )
        .await?;
    }
    Ok(())
}

async fn seed_viscosidad(pool: &SqlitePool) -> anyhow::Result<()> {
    let fresh_visc = seed_practice(
        pool,
        "viscosidad",
        &[
            qty("R", "Radio de la esfera", "m", false, "longitud"),
            qty_replicas("t", "Tiempo de caida", "s", "tiempo", 5),
            qty_shared("dx", "Distancia recorrida", "m", "longitud"),
            qty_given("rho_e", "Densidad del acero", "kg/m3", "densidad"),
            qty_shared("rho_f", "Densidad de la glicerina", "kg/m3", "densidad"),
            qty_given("g", "Aceleracion de la gravedad", "m/s2", "aceleracion"),
            qty_shared(
                "Temp",
                "Temperatura de la glicerina (referencia)",
                "C",
                "temperatura",
            ),
        ],
        &[res_final(
            "mu",
            "Viscosidad de la glicerina",
            "Pa.s",
            "(rho_e - rho_f)*2*g/(9*slope)",
        )],
    )
    .await?;
    if fresh_visc {
        create_point_result(
            pool,
            "viscosidad",
            PointResultInput {
                symbol: "Re".into(),
                name: "Numero de Reynolds".into(),
                unit: "".into(),
                formula: "rho_f*(dx/t)*2*R/mu".into(),
            },
        )
        .await?;
    }
    sqlx::query(
        "UPDATE practice_quantities SET is_given = 1 \
         WHERE practice_id = 'viscosidad' AND symbol = 'rho_e' AND is_given = 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_fluidos2(pool: &SqlitePool) -> anyhow::Result<()> {
    let fresh_f2 = seed_practice(
        pool,
        "fluidos-2",
        &[
            qty("h", "Altura de la columna", "m", false, "longitud"),
            qty("t", "Tiempo de escurrimiento", "s", false, "tiempo"),
            qty_shared("h_max", "Altura inicial (maxima)", "m", "longitud"),
            qty_shared("R_cap", "Radio del capilar", "m", "longitud"),
            qty_shared("L_cap", "Longitud del capilar", "m", "longitud"),
            qty_shared("R_recip", "Radio del recipiente", "m", "longitud"),
            qty_given("g", "Aceleracion de la gravedad", "m/s2", "aceleracion"),
            qty_shared("rho", "Densidad del agua", "kg/m3", "densidad"),
            no_u(qty_given(
                "mu_agua",
                "Viscosidad del agua (de tabla segun T)",
                "Pa.s",
                "viscosidad",
            )),
            no_u(qty_given(
                "kp",
                "Factor geometrico K (def. 0.78)",
                "",
                "adimensional",
            )),
            qty_shared(
                "Temp",
                "Temperatura del agua (referencia)",
                "C",
                "temperatura",
            ),
        ],
        &[res_final(
            "M_medio",
            "Coeficiente medio de perdidas",
            "",
            "2*g*(slope*R_cap^2/(2*R_recip^2))^2 - 2",
        )],
    )
    .await?;
    if fresh_f2 {
        for input in [
            AggregateInput {
                symbol: "Re_max".into(),
                name: "Numero de Reynolds maximo".into(),
                unit: "".into(),
                formula:
                    "2*rho*((h_first - h_first2)/(t_first2 - t_first))*(R_recip^2/(mu_agua*R_cap))"
                        .into(),
                is_final: true,
            },
            AggregateInput {
                symbol: "Re_min".into(),
                name: "Numero de Reynolds minimo".into(),
                unit: "".into(),
                formula:
                    "2*rho*((h_last2 - h_last)/(t_last - t_last2))*(R_recip^2/(mu_agua*R_cap))"
                        .into(),
                is_final: true,
            },
            AggregateInput {
                symbol: "Re_medio".into(),
                name: "Numero de Reynolds medio".into(),
                unit: "".into(),
                formula: "(Re_max + Re_min)/2".into(),
                is_final: true,
            },
            AggregateInput {
                symbol: "M_teorico".into(),
                name: "Coeficiente de perdidas teorico".into(),
                unit: "".into(),
                formula: "kp + 4*(L_cap/(2*R_cap))*(16/Re_medio)".into(),
                is_final: true,
            },
        ] {
            create_aggregate(pool, "fluidos-2", input).await?;
        }
    }
    sqlx::query(
        "UPDATE practice_quantities SET has_uncertainty = 0, is_given = 1 \
         WHERE practice_id = 'fluidos-2' AND symbol IN ('mu_agua', 'kp') \
         AND (has_uncertainty = 1 OR is_given = 0)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE practice_aggregates SET is_final = 1 \
         WHERE practice_id = 'fluidos-2' \
         AND symbol IN ('Re_max', 'Re_min', 'Re_medio', 'M_teorico') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_filtros(pool: &SqlitePool) -> anyhow::Result<()> {
    if !quantity_missing(pool, "filtros", "L").await? {
        let given_r: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM practice_quantities \
             WHERE practice_id = 'filtros' AND symbol = 'R' AND is_given = 1",
        )
        .fetch_one(pool)
        .await?;
        let missing_exp = quantity_missing(pool, "filtros", "fpasaje_exp").await?;
        if given_r.0 > 0 || missing_exp {
            let mut tx = pool.begin().await?;
            wipe_practice_definition(&mut tx, "filtros").await?;
            tx.commit().await?;
        }
    }
    let fresh_filtros = seed_practice(
        pool,
        "filtros",
        &[
            qty("f", "Frecuencia", "Hz", false, "frecuencia"),
            qty("VRpp", "Tension pico a pico en R", "V", false, "tension"),
            qty(
                "Vgpp",
                "Tension pico a pico del generador",
                "V",
                false,
                "tension",
            ),
            qty("a", "Semieje mayor de Lissajous", "V", false, "tension"),
            qty("b", "Semieje menor de Lissajous", "V", false, "tension"),
            qty_shared("R", "Resistencia", "ohm", "resistencia"),
            qty_shared("C1", "Capacitor C_1", "F", "capacitancia"),
            qty_shared("C2", "Capacitor C_2", "F", "capacitancia"),
            qty_shared(
                "fpasaje_exp",
                "Frecuencia de pasaje experimental",
                "Hz",
                "frecuencia",
            ),
            qty_shared(
                "fbloqueo_exp",
                "Frecuencia de bloqueo experimental",
                "Hz",
                "frecuencia",
            ),
            qty_given("L", "Inductor", "H", "inductancia"),
        ],
        &[
            res_final(
                "fpasaje",
                "Frecuencia de pasaje teorica",
                "Hz",
                "1/(2*pi*math::sqrt(L*(C1+C2)))",
            ),
            res_final(
                "fbloqueo",
                "Frecuencia de bloqueo teorica",
                "Hz",
                "1/(2*pi*math::sqrt(L*C2))",
            ),
        ],
    )
    .await?;
    if fresh_filtros {
        for (sym, name, unit, formula) in [
            ("omega", "Frecuencia angular", "rad/s", "2*pi*f"),
            ("razon", "Razon de amplitud VR/Vg", "", "VRpp/Vgpp"),
            ("phi", "Desfasaje", "rad", "math::asin(b/a)"),
        ] {
            create_intermediate(
                pool,
                "filtros",
                IntermediateInput {
                    symbol: sym.into(),
                    name: name.into(),
                    unit: unit.into(),
                    formula: formula.into(),
                },
            )
            .await?;
        }
        for (x, y, x_log) in [("omega", "razon", true), ("omega", "phi", true)] {
            create_curve(
                pool,
                "filtros",
                CurveInput {
                    x_formula: x.into(),
                    y_formula: y.into(),
                    x_log,
                },
            )
            .await?;
        }
    }
    sqlx::query(
        "UPDATE practice_quantities SET is_given = 0 \
         WHERE practice_id = 'filtros' AND symbol IN ('R', 'C1', 'C2') AND is_given = 1",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE practice_results SET is_final = 1 \
         WHERE practice_id = 'filtros' AND symbol IN ('fpasaje', 'fbloqueo') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    for (sym, name) in [
        ("C1", "Capacitor C_1"),
        ("C2", "Capacitor C_2"),
        ("fpasaje_exp", "Frecuencia de pasaje experimental"),
        ("fbloqueo_exp", "Frecuencia de bloqueo experimental"),
    ] {
        sqlx::query(
            "UPDATE practice_quantities SET name = ?2 \
             WHERE practice_id = 'filtros' AND symbol = ?1 AND name != ?2",
        )
        .bind(sym)
        .bind(name)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub(super) async fn seed_ca_rlc(pool: &SqlitePool) -> anyhow::Result<()> {
    let omega = "(2*pi*f_trabajo)";
    let xl = format!("({omega}*L)");
    let xc = format!("(1/({omega}*C))");
    let z = format!("math::sqrt(R*R + (({xl})-({xc}))*(({xl})-({xc})))");
    let i_teo = format!("(Vg/({z}))");
    let phi_teo = format!("math::atan((({xl})-({xc}))/R)");
    seed_practice(
        pool,
        "ca-rlc",
        &[
            qty_shared("R", "Resistencia", "ohm", "resistencia"),
            qty_shared("C", "Capacitor", "F", "capacitancia"),
            qty_given("L", "Inductor", "H", "inductancia"),
            qty_shared("Vg", "Voltaje en el generador", "V", "voltaje"),
            qty_shared(
                "f_res_exp",
                "Frecuencia de resonancia experimental",
                "Hz",
                "frecuencia",
            ),
            qty_shared("f_trabajo", "Frecuencia de trabajo", "Hz", "frecuencia"),
            qty_shared("VRpp", "Voltaje en la resistencia", "V", "voltaje"),
            qty_shared("VCpp", "Voltaje en el capacitor", "V", "voltaje"),
            qty_shared("VLpp", "Voltaje en el inductor", "V", "voltaje"),
            qty_shared("a_R", "Semieje mayor de Lissajous - R", "V", "voltaje"),
            qty_shared("b_R", "Semieje menor de Lissajous - R", "V", "voltaje"),
            qty_shared("a_C", "Semieje mayor de Lissajous - C", "V", "voltaje"),
            qty_shared("b_C", "Semieje menor de Lissajous - C", "V", "voltaje"),
            qty_shared("a_L", "Semieje mayor de Lissajous - L", "V", "voltaje"),
            qty_shared("b_L", "Semieje menor de Lissajous - L", "V", "voltaje"),
        ],
        &[
            res("XL", "Reactancia inductiva", "ohm", &xl),
            res("XC", "Reactancia capacitiva", "ohm", &xc),
            res("Z", "Impedancia", "ohm", &z),
            res_final(
                "f_res",
                "Frecuencia de resonancia teorica",
                "Hz",
                "1/(2*pi*math::sqrt(L*C))",
            ),
            res_final("I_teo", "Corriente teorica", "A", &i_teo),
            res_final(
                "VR_teo",
                "Voltaje teorico en la resistencia",
                "V",
                &format!("({i_teo})*R"),
            ),
            res_final(
                "VR_exp",
                "Voltaje experimental en la resistencia",
                "V",
                "VRpp/2",
            ),
            res_final(
                "VC_teo",
                "Voltaje teorico en el capacitor",
                "V",
                &format!("({i_teo})*({xc})"),
            ),
            res_final(
                "VC_exp",
                "Voltaje experimental en el capacitor",
                "V",
                "VCpp/2",
            ),
            res_final(
                "VL_teo",
                "Voltaje teorico en el inductor",
                "V",
                &format!("({i_teo})*({xl})"),
            ),
            res_final(
                "VL_exp",
                "Voltaje experimental en el inductor",
                "V",
                "VLpp/2",
            ),
            res_final(
                "phiR_teo",
                "Desfasaje teorico en R",
                "°",
                &format!("(-({phi_teo}))*180/pi"),
            ),
            res_final(
                "phiR_exp",
                "Desfasaje experimental en R",
                "°",
                "math::asin(b_R/a_R)*180/pi",
            ),
            res_final(
                "phiC_teo",
                "Desfasaje teorico en C",
                "°",
                &format!("(-({phi_teo})-pi/2)*180/pi"),
            ),
            res_final(
                "phiC_exp",
                "Desfasaje experimental en C",
                "°",
                "math::asin(b_C/a_C)*180/pi",
            ),
            res_final(
                "phiL_teo",
                "Desfasaje teorico en L",
                "°",
                &format!("(-({phi_teo})+pi/2)*180/pi"),
            ),
            res_final(
                "phiL_exp",
                "Desfasaje experimental en L",
                "°",
                "math::asin(b_L/a_L)*180/pi",
            ),
        ],
    )
    .await?;
    Ok(())
}

const MOHR_RANURAS: usize = 9;

const HIDRO_MEDIDAS: usize = 3;

pub(super) async fn seed_hidrostatica(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut quantities = vec![
        qty_shared("m_goma", "Masa de la goma", "g", "masa"),
        qty_shared("rho_f", "Densidad del fluido", "g/cm3", "densidad"),
        no_u(qty_given(
            "g_ac",
            "Aceleracion gravitatoria",
            "m/s2",
            "aceleracion",
        )),
        with_default(
            qty_given("b_E", "Brazo fijo del empuje", "cm", "longitud"),
            10.0,
        ),
    ];
    for i in 1..=MOHR_RANURAS {
        quantities.push(with_default(
            qty_given(
                &format!("b{i}"),
                &format!("Brazo de la ranura {i}"),
                "cm",
                "longitud",
            ),
            i as f64,
        ));
    }
    for k in 1..=HIDRO_MEDIDAS {
        for i in 1..=MOHR_RANURAS {
            quantities.push(with_default(
                qty_shared(
                    &format!("m{k}_{i}"),
                    &format!("Medida {k} - masa en la ranura {i}"),
                    "g",
                    "masa",
                ),
                0.0,
            ));
        }
    }
    quantities.push(qty_shared("l", "Base del marco", "cm", "longitud"));
    quantities.push(with_default(
        no_u(qty_given("d", "Brazo fijo del marco", "cm", "longitud")),
        10.0,
    ));
    for k in 1..=HIDRO_MEDIDAS {
        quantities.push(qty_shared(
            &format!("mw{k}"),
            &format!("Medida {k} - masa de la pesa (Wilhelmy)"),
            "g",
            "masa",
        ));
        quantities.push(slots(
            no_u(qty_given(
                &format!("dw{k}"),
                &format!("Medida {k} - ranura de la pesa (cm)"),
                "cm",
                "longitud",
            )),
            MOHR_RANURAS as i64,
        ));
    }

    let torque = |k: usize| {
        (1..=MOHR_RANURAS)
            .map(|i| format!("m{k}_{i}*b{i}"))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let empuje = |k: usize| format!("(({})/b_E)*g_ac/1000", torque(k));
    let densidad = |k: usize| format!("rho_f*m_goma*b_E*1000/({})", torque(k));
    let gamma = |k: usize| format!("mw{k}*g_ac*dw{k}*100/(2*l*d*1000)");
    let promedio = |partes: [String; HIDRO_MEDIDAS]| {
        format!("(({}) + ({}) + ({}))/3", partes[0], partes[1], partes[2])
    };

    let mut results = Vec::new();
    for k in 1..=HIDRO_MEDIDAS {
        results.push(res(
            &format!("E{k}"),
            &format!("Empuje - medida {k}"),
            "N",
            &empuje(k),
        ));
        results.push(res(
            &format!("rho{k}"),
            &format!("Densidad de la goma - medida {k}"),
            "kg/m3",
            &densidad(k),
        ));
    }
    results.push(res_final(
        "E_medio",
        "Empuje promedio",
        "N",
        &promedio([empuje(1), empuje(2), empuje(3)]),
    ));
    results.push(res_final(
        "rho_medio",
        "Densidad de la goma promedio",
        "kg/m3",
        &promedio([densidad(1), densidad(2), densidad(3)]),
    ));
    for k in 1..=HIDRO_MEDIDAS {
        results.push(res_no_u(res(
            &format!("gamma{k}"),
            &format!("Tension superficial - medida {k}"),
            "N/m",
            &gamma(k),
        )));
    }
    results.push(res_no_u(res_final(
        "gamma_medio",
        "Tension superficial promedio",
        "N/m",
        &promedio([gamma(1), gamma(2), gamma(3)]),
    )));

    seed_practice(pool, "hidrostatica", &quantities, &results).await?;
    Ok(())
}

const HIDRO_RESET: &str = "2026-11-hidrostatica-ranuras";

const CA_RLC_FIX: &str = "2026-11-ca-rlc-correcciones";

pub(super) async fn reset_legacy_hidrostatica(pool: &SqlitePool) -> anyhow::Result<()> {
    let done: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
        .bind(HIDRO_RESET)
        .fetch_one(pool)
        .await?;
    if done.0 > 0 {
        return Ok(());
    }
    let seeded: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'hidrostatica'",
    )
    .fetch_one(pool)
    .await?;
    let seeded = seeded.0 > 0;
    let mut tx = pool.begin().await?;
    if seeded {
        for sql in [
            "DELETE FROM submission_measurements WHERE submission_id IN \
             (SELECT id FROM submissions WHERE practice_id = 'hidrostatica')",
            "DELETE FROM submission_student_results WHERE submission_id IN \
             (SELECT id FROM submissions WHERE practice_id = 'hidrostatica')",
            "DELETE FROM report_members WHERE submission_id IN \
             (SELECT id FROM submissions WHERE practice_id = 'hidrostatica')",
        ] {
            sqlx::query(sql).execute(&mut *tx).await?;
        }
        let submissions = sqlx::query("DELETE FROM submissions WHERE practice_id = 'hidrostatica'")
            .execute(&mut *tx)
            .await?
            .rows_affected();
        wipe_practice_definition(&mut tx, "hidrostatica").await?;
        tracing::warn!(
            submissions,
            "Hidrostática tenía la forma vieja: se borró (definición y entregas) para resembrarla"
        );
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(HIDRO_RESET)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn fix_ca_rlc_labels(pool: &SqlitePool) -> anyhow::Result<()> {
    let already: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
        .bind(CA_RLC_FIX)
        .fetch_one(pool)
        .await?;
    if already.0 > 0 {
        return Ok(());
    }
    if !result_missing(pool, "ca-rlc", "I_exp").await? {
        sqlx::query(
            "DELETE FROM practice_results WHERE practice_id = 'ca-rlc' AND symbol = 'I_exp'",
        )
        .execute(pool)
        .await?;
    }

    let f_trabajo: Option<(i64,)> = sqlx::query_as(
        "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_trabajo'",
    )
    .fetch_optional(pool)
    .await?;
    if let (true, Some(f_trabajo_pos)) = (
        quantity_missing(pool, "ca-rlc", "f_res_exp").await?,
        f_trabajo,
    ) {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "UPDATE practice_quantities SET position = position + 1 \
             WHERE practice_id = 'ca-rlc' AND position >= ?1",
        )
        .bind(f_trabajo_pos.0)
        .execute(&mut *tx)
        .await?;
        insert_quantity(
            &mut tx,
            "ca-rlc",
            f_trabajo_pos.0,
            &qty_shared(
                "f_res_exp",
                "Frecuencia de resonancia experimental",
                "Hz",
                "frecuencia",
            ),
        )
        .await?;
        tx.commit().await?;
    }

    let mut tx = pool.begin().await?;
    {
        let stray_position: Option<(i64, i64)> = sqlx::query_as(concat!(
            "SELECT fre.position, ft.position ",
            "FROM practice_quantities fre, practice_quantities ft ",
            "WHERE fre.practice_id = 'ca-rlc' AND fre.symbol = 'f_res_exp' ",
            "AND ft.practice_id = 'ca-rlc' AND ft.symbol = 'f_trabajo' ",
            "AND fre.position > ft.position",
        ))
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((f_res_exp_pos, f_trabajo_pos)) = stray_position {
            sqlx::query(concat!(
                "UPDATE practice_quantities SET position = position + 1 ",
                "WHERE practice_id = 'ca-rlc' AND position >= ?1 AND position < ?2",
            ))
            .bind(f_trabajo_pos)
            .bind(f_res_exp_pos)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE practice_quantities SET position = ?1 WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
            )
            .bind(f_trabajo_pos)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;

    for symbol in ["a_R", "b_R", "a_C", "b_C", "a_L", "b_L"] {
        sqlx::query(
            "UPDATE practice_quantities SET name = REPLACE(REPLACE(name, ' (a)', ''), ' (b)', '') \
             WHERE practice_id = 'ca-rlc' AND symbol = ?1",
        )
        .bind(symbol)
        .execute(pool)
        .await?;
    }

    let quantity_renames = [
        ("Vg", "Voltaje en el generador"),
        ("VRpp", "Voltaje en la resistencia"),
        ("VCpp", "Voltaje en el capacitor"),
        ("VLpp", "Voltaje en el inductor"),
        ("a_R", "Semieje mayor de Lissajous - R"),
        ("b_R", "Semieje menor de Lissajous - R"),
        ("a_C", "Semieje mayor de Lissajous - C"),
        ("b_C", "Semieje menor de Lissajous - C"),
        ("a_L", "Semieje mayor de Lissajous - L"),
        ("b_L", "Semieje menor de Lissajous - L"),
    ];
    for (symbol, new_name) in quantity_renames {
        sqlx::query(
            "UPDATE practice_quantities SET name = ?1, quantity = 'voltaje' \
             WHERE practice_id = 'ca-rlc' AND symbol = ?2 AND quantity = 'tension'",
        )
        .bind(new_name)
        .bind(symbol)
        .execute(pool)
        .await?;
    }

    let result_renames = [
        ("VR_teo", "Voltaje teorico en la resistencia"),
        ("VR_exp", "Voltaje experimental en la resistencia"),
        ("VC_teo", "Voltaje teorico en el capacitor"),
        ("VC_exp", "Voltaje experimental en el capacitor"),
        ("VL_teo", "Voltaje teorico en el inductor"),
        ("VL_exp", "Voltaje experimental en el inductor"),
    ];
    for (symbol, new_name) in result_renames {
        sqlx::query(
            "UPDATE practice_results SET name = ?1 WHERE practice_id = 'ca-rlc' AND symbol = ?2",
        )
        .bind(new_name)
        .bind(symbol)
        .execute(pool)
        .await?;
    }

    let omega = "(2*pi*f_trabajo)";
    let xl = format!("({omega}*L)");
    let xc = format!("(1/({omega}*C))");
    let phi_teo = format!("math::atan((({xl})-({xc}))/R)");
    let phase_updates = [
        ("phiR_teo", format!("(-({phi_teo}))*180/pi")),
        ("phiR_exp", "math::asin(b_R/a_R)*180/pi".to_string()),
        ("phiC_teo", format!("(-({phi_teo})-pi/2)*180/pi")),
        ("phiC_exp", "math::asin(b_C/a_C)*180/pi".to_string()),
        ("phiL_teo", format!("(-({phi_teo})+pi/2)*180/pi")),
        ("phiL_exp", "math::asin(b_L/a_L)*180/pi".to_string()),
    ];
    for (symbol, formula) in phase_updates {
        sqlx::query(
            "UPDATE practice_results SET formula = ?1, unit = '°' \
             WHERE practice_id = 'ca-rlc' AND symbol = ?2 AND unit = 'rad'",
        )
        .bind(formula)
        .bind(symbol)
        .execute(pool)
        .await?;
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(CA_RLC_FIX)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

const P3_UNIFIED: &str = "2026-11-p3-unificada";

pub(super) async fn unify_p3(pool: &SqlitePool) -> anyhow::Result<()> {
    let (done,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
        .bind(P3_UNIFIED)
        .fetch_one(pool)
        .await?;
    if done > 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let (legacy,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practices WHERE id = 'p3-relajacion-desfasaje'")
            .fetch_one(&mut *tx)
            .await?;
    if legacy > 0 {
        sqlx::query(
            "DELETE FROM submissions \
             WHERE practice_id IN ('p3-relajacion', 'p3-relajacion-desfasaje')",
        )
        .execute(&mut *tx)
        .await?;
        for table in [
            "course_practices",
            "practice_subgroups",
            "practice_table_assignments",
        ] {
            sqlx::query(&format!(
                "UPDATE OR IGNORE {table} SET practice_id = 'p3-relajacion' \
                 WHERE practice_id = 'p3-relajacion-desfasaje'"
            ))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM practices WHERE id = 'p3-relajacion-desfasaje'")
            .execute(&mut *tx)
            .await?;
        wipe_practice_definition(&mut tx, "p3-relajacion").await?;
        for table in ["practice_sections", "practice_parts"] {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE practice_id = 'p3-relajacion'"
            ))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "UPDATE practices SET analysis_kind = 'regresion_lineal', x_formula = '2*pi*f', \
             y_formula = 'b / math::sqrt(a*a - b*b)', description = ?1 \
             WHERE id = 'p3-relajacion'",
        )
        .bind(crate::db::P3_DESCRIPTION)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(P3_UNIFIED)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn seed_definitions(pool: &SqlitePool) -> anyhow::Result<()> {
    unify_p3(pool).await?;
    seed_p1_estadistica(pool).await?;
    seed_p3_relajacion(pool).await?;
    seed_p2_cc(pool).await?;
    seed_fluidos1(pool).await?;
    seed_viscosidad(pool).await?;
    seed_fluidos2(pool).await?;
    seed_filtros(pool).await?;
    seed_ca_rlc(pool).await?;
    fix_ca_rlc_labels(pool).await?;
    reset_legacy_hidrostatica(pool).await?;
    seed_hidrostatica(pool).await?;
    super::layout::migrate_legacy_layout(pool).await?;
    super::layout::migrate_shared_results_to_parts(pool).await?;
    Ok(())
}
