use super::crud::{
    create_aggregate, create_curve, create_intermediate, create_point_result, insert_quantity,
    insert_result,
};
use super::{
    AggregateInput, CurveInput, IntermediateInput, PointResultInput, QuantityInput, ResultInput,
};
use sqlx::SqlitePool;

/// Construye un `QuantityInput` (magnitud de entrada) para el seed de definiciones.
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
    }
}

/// Construye un `QuantityInput` para un dato dado por la cátedra (valor ± U, sin réplicas).
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
    }
}

/// Igual que [`qty_given`], pero sin campo de incertidumbre: el formulario pide solo "Valor" (sin
/// instrumento ni U), y se computa con U = 0. Para datos de tabla que no tienen incertidumbre
/// propia (p. ej. un tiempo leído de una tabla de referencia).
fn no_u(mut q: QuantityInput) -> QuantityInput {
    q.has_uncertainty = false;
    q
}

/// Marca una magnitud como opcional: puede quedar sin lecturas sin bloquear el envío.
fn opt(mut q: QuantityInput) -> QuantityInput {
    q.optional = true;
    q
}

/// Magnitud medida **por punto con réplicas** (regresión/curva): grilla de `replicas` por punto.
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
    }
}

/// Escalar **compartido** medido una sola vez (no por punto, no dato de cátedra): p. ej. la
/// densidad medida con densímetro al final de la práctica.
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
    }
}

/// Construye un `ResultInput` (mensurando derivado) para el seed de definiciones.
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

/// Igual que [`res`], pero marcado como resultado central que el alumno debe entregar.
fn res_final(symbol: &str, name: &str, unit: &str, formula: &str) -> ResultInput {
    ResultInput {
        is_final: true,
        ..res(symbol, name, unit, formula)
    }
}

/// Igual que [`res`]/[`res_final`], pero oculta la ±U en toda la UI (reemplaza el hack
/// `RESULTS_WITHOUT_U` del frontend). El valor propagado de fondo no cambia, solo su display.
fn res_no_u(mut r: ResultInput) -> ResultInput {
    r.has_uncertainty = false;
    r
}

/// Siembra la definición de una práctica (magnitudes + mensurandos). Idempotente: no hace nada si
/// la práctica ya tiene magnitudes. Devuelve `true` si la sembró ahora (`false` si ya existía),
/// para que el llamador siembre los extras (intermedias/derivadas) solo en el alta fresca y no los
/// re-cree si el docente los borró luego.
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

/// `true` si la práctica no tiene una magnitud con ese símbolo. Para migraciones puntuales que
/// agregan símbolos nuevos a una práctica ya sembrada (`seed_practice` es idempotente y no
/// re-siembra), evitando duplicar el `INSERT` en una base que ya los tiene.
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

/// Igual que [`quantity_missing`], para mensurandos derivados.
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

/// Siembra P1 (ver [`seed_definitions`]).
async fn seed_p1_estadistica(pool: &SqlitePool) -> anyhow::Result<()> {
    // P1 — Tratamiento estadístico de datos (péndulo simple), con 3 operadores independientes:
    // cada uno mide su propia serie de períodos (T1/T2/T3, cronómetro, sin cruzar datos entre
    // operadores) y tiene su propio g1/g2/g3 = 4*pi^2*L/T{n}^2. Operador 1 obligatorio; 2 y 3
    // opcionales (el alumno puede no cargarlos). L (cátedra), t_med y los mensurandos gamma/Q son
    // únicos para toda la práctica (no por operador). t_med ("t_1/2") es un dato de tabla sin
    // incertidumbre propia: solo pide el campo "Valor" (`no_u`, sin instrumento ni U). gamma y Q
    // se muestran sin ±U (`res_no_u`) aunque Q sí propaga, de fondo, la incertidumbre real del
    // período (usa el del operador 1 — ver el nombre del mensurando).
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
    // Migración de forma: la definición original tenía un único T/g compartidos; ahora son T1/T2/T3
    // y g1/g2/g3 por operador, más el flag `has_uncertainty` en t_med/gamma/Q. `seed_practice` es
    // idempotente y no re-siembra sobre una base ya sembrada, así que las instalaciones existentes
    // necesitan este backfill puntual (no-op en instalaciones nuevas, que ya siembran la forma final
    // arriba). Cada bloque corre una única vez, guardado por el símbolo nuevo que da de alta: una
    // vez que T1/g1 existen, no se vuelve a tocar (así no se pisan ediciones del admin sobre
    // has_uncertainty/optional/formula hechas después de la migración).
    if quantity_missing(pool, "p1-estadistica", "T1").await? {
        // Una sola transacción: si el proceso muere a mitad de camino (p. ej. despues de borrar
        // `T` pero antes de insertar T2/T3), `quantity_missing(pool, "T1")` ya daria `false` en
        // el siguiente boot (T1 si se llego a insertar) y el bloque no se volveria a correr,
        // dejando la migracion incompleta para siempre.
        let mut tx = pool.begin().await?;
        // Borra primero las mediciones que referencian la magnitud vieja: `submission_measurements
        // .quantity_id` tiene FK a `practice_quantities(id)` sin `ON DELETE CASCADE`, así que
        // borrar la magnitud con mediciones reales cargadas violaría la constraint (con
        // `foreign_keys` activo) y tiraría el boot abajo. Si hubiera entregas reales sobre `T`,
        // se descartan sus mediciones en vez de eso.
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
        // t_med sin instrumento/U, T2/T3 opcionales: forma vieja de la base no tenía estos flags.
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
        // Misma razón que la migración de arriba: todo o nada, para no quedar con g1 insertado
        // pero g2/g3 faltantes si el proceso muere a mitad de camino.
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
        // gamma/Q sin ±U, y Q pasa a referenciar T1 (antes T): forma vieja de la base no tenía
        // `has_uncertainty` ni el operador explícito en la fórmula/nombre.
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
    // gamma/Q pasaron a ser resultado final después del alta inicial de la práctica: invariante que
    // se re-aplica en cada boot (no solo en la migración de forma de arriba) para autocurar bases
    // donde haya quedado en `is_final = 0` por cualquier motivo.
    sqlx::query(
        "UPDATE practice_results SET is_final = 1 \
         WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    // `operator_count` (Motor D) es de una epoca anterior a T1/T2/T3: dividia UNA magnitud
    // repetida ("T") en N series, una por operador. Ahora cada operador ya es un simbolo propio
    // (T1/T2/T3, con su propio tab y su propio g_i), asi que Motor D queda incompatible: si
    // quedo seteado (p. ej. desde el admin, antes de este rediseño) vuelve a tratar T1/T2/T3 como
    // si fueran 3 series del mismo operador, mostrando 3 cronometros por tab en vez de 1 y
    // triplicando los mensurandos derivados. Se autocura en cada boot.
    sqlx::query(
        "UPDATE practices SET operator_count = NULL \
         WHERE id = 'p1-estadistica' AND operator_count IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Siembra P3 parte 1 (ver [`seed_definitions`]).
async fn seed_p3_relajacion(pool: &SqlitePool) -> anyhow::Result<()> {
    // P3 — Relajación exponencial (parte 1, determinación directa de tau en un RC serie).
    // tau_teorico = (R + Rint)*C ; tau_exp = t_medio/ln2 (porque t_1/2 = tau*ln2).
    // Unidades SI (ohm, F, s) para que tau salga en segundos. Tipo A despreciable -> medida unica.
    // (La parte 2 por desfasaje es regresion_lineal y se agregara cuando este implementada.)
    seed_practice(
        pool,
        "p3-relajacion",
        &[
            qty("R", "Resistencia", "ohm", false, "resistencia"),
            // Rint es un dato entregado por la cátedra (valor ± U), no lo mide el alumno.
            qty_given(
                "Rint",
                "Resistencia interna de la fuente",
                "ohm",
                "resistencia",
            ),
            qty("C", "Capacitancia", "F", false, "capacitancia"),
            // Periodo de la onda cuadrada de trabajo (se registra; debe permitir ver ~5*tau
            // en el semiperiodo de descarga). No entra en las formulas, queda como dato medido.
            qty("T_oc", "Periodo de la onda cuadrada", "s", false, "tiempo"),
            qty(
                "tmedio",
                "Tiempo de semidescarga (t1/2)",
                "s",
                false,
                "tiempo",
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
        ],
    )
    .await?;
    Ok(())
}

/// Siembra P2-CC (ver [`seed_definitions`]).
async fn seed_p2_cc(pool: &SqlitePool) -> anyhow::Result<()> {
    // P2 — Corriente continua unificada: una sola entrega con tres partes tematicas.
    // Escalares compartidos: R1, R2 y R3 se miden UNA vez (ohmetro) y valen para toda la
    // practica; Vg y RA pueden cambiar entre partes, asi que se miden por parte (sufijos
    // _s = serie, _p = paralelo, _c = curva de potencia). Los voltajes VRi_s / VRi_p se miden
    // con multimetro y se comparan con las teoricas VRi_s_t / VRi_p_t (resultados finales que
    // el alumno calcula a mano con propagacion). Por punto: R (carga variable) e I; intermedia
    // P = I^2*R y curva P vs R. Los finales experimentales de potencia (P_max_e / RP_max_e)
    // usan los alias de extremos del camino curva (`P_max`, `R_at_P_max`) con U = 0; por eso
    // sus formulas no son editables desde la UI admin (check_formula no conoce los alias).
    // Migración de forma: mientras se desarrollaba la unificación (#43) algunas bases quedaron
    // sembradas con una forma intermedia de p2-cc (símbolos con sufijo _serie/_paralelo/_potencia,
    // p. ej. `Vg_serie`/`RA_serie`, en vez de los `_s`/`_p`/`_c` actuales). `seed_practice` es
    // idempotente y no resiembra sobre una base que ya tiene filas, así que esas bases quedan
    // desincronizadas de PRACTICE_SECTIONS (constants.js) para siempre: el front no encuentra los
    // símbolos esperados, no arma `data-section` y las tabs Serie/Paralelo/Potencia dejan de
    // separar sus campos. No hay mediciones reales bajo esa forma intermedia (era de desarrollo),
    // así que en vez de renombrar símbolo a símbolo se limpia todo p2-cc y se deja que el seed de
    // abajo lo siembre de cero con la forma final.
    if !quantity_missing(pool, "p2-cc", "R").await?
        && quantity_missing(pool, "p2-cc", "Vg_s").await?
    {
        // Una sola transacción: si el proceso muere a mitad de camino, no queda un estado
        // parcial (p. ej. `practice_quantities` ya vacía pero `practice_results` todavía con
        // las filas viejas, que chocarían con `UNIQUE(practice_id, symbol)` al resembrar).
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM submission_measurements WHERE quantity_id IN \
             (SELECT id FROM practice_quantities WHERE practice_id = 'p2-cc')",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM practice_quantities WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM practice_results WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM practice_curves WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM practice_intermediates WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
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
            // RA es un dato de tabla segun la escala del amperimetro, no se mide: va como dato
            // dado por catedra (valor +/- U), igual en las tres partes.
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

/// Siembra P3 parte 2 (ver [`seed_definitions`]).
async fn seed_p3_relajacion_desfasaje(pool: &SqlitePool) -> anyhow::Result<()> {
    // P3 — parte 2 (desfasaje por figura de Lissajous). El alumno carga una serie de puntos
    // con f, a y b; las fórmulas de eje (en `practices.x_formula`/`y_formula`) derivan
    // x = 2*pi*f (= omega) y y = b/sqrt(a^2 - b^2) (= tg phi). La pendiente del ajuste es
    // RC = tau, que se referencia con el símbolo especial `slope`.
    seed_practice(
        pool,
        "p3-relajacion-desfasaje",
        &[
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
        &[res_final("tau", "Constante de tiempo RC", "s", "slope")],
    )
    .await?;
    Ok(())
}

/// Siembra Fluidos I (ver [`seed_definitions`]).
async fn seed_fluidos1(pool: &SqlitePool) -> anyhow::Result<()> {
    // Fluidos I — viscosidad por Hagen-Poiseuille. Por altura (punto) se miden V y t con 2
    // réplicas; Q = V/t (intermedia, promedio por punto). Ejes: 1/Q vs h/Q^2 (set en seed_practices).
    // Escalares compartidos: R, L, g (cátedra) y rho (medida única). `Temp` se registra solo como
    // referencia (para buscar la viscosidad de tablas a esa temperatura): no entra en ninguna
    // fórmula y va sin incertidumbre. Mensurando mu desde la pendiente; Reynolds por corrida.
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
    // Intermedia Q (Motor C) y derivada por corrida Reynolds (Motor E): solo en el alta fresca,
    // para no re-crearlas si el docente las edita/borra luego (`analysis_kind`/fórmulas se preservan
    // en `seed_practices`).
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

/// Siembra Viscosidad (ver [`seed_definitions`]).
async fn seed_viscosidad(pool: &SqlitePool) -> anyhow::Result<()> {
    // Viscosidad (Stokes) — ajuste v_lim vs R^2 (ejes en seed_practices: x=R^2, y=dx/t). Por esfera
    // (punto): R (un valor) y t (5 réplicas → Motor A promedia → t medio, así y = dx/t = v_lim).
    // Escalares compartidos: dx, rho_e, rho_f (medida única), g (cátedra); Temp de referencia.
    // Mensurando mu = (rho_e - rho_f)*2*g/(9*slope); Reynolds por corrida. Sin intermedia.
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
    // Auto-curación: rho_e (densidad del acero) es un dato dado con incertidumbre, no una medida
    // con instrumento. Re-aplica en cada boot para bases sembradas antes del cambio.
    sqlx::query(
        "UPDATE practice_quantities SET is_given = 1 \
         WHERE practice_id = 'viscosidad' AND symbol = 'rho_e' AND is_given = 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Siembra Fluidos II (ver [`seed_definitions`]).
async fn seed_fluidos2(pool: &SqlitePool) -> anyhow::Result<()> {
    // Fluidos II — descarga de un recipiente por un capilar. Por punto: h (altura) y t (tiempo, con
    // t=0 en la altura maxima). Ejes (en seed_practices): x = sqrt(h_max) - sqrt(h), y = t. La
    // pendiente da M_medio = 2*g*(slope*R_cap^2/(2*R_recip^2))^2 - 2 (coef. medio de perdidas).
    // Escalares compartidos: R_cap, L_cap, R_recip (medidos con regla, con incertidumbre), g
    // (catedra), rho (densimetro al final), mu_agua (viscosidad del agua de tabla segun T), kp
    // (factor geometrico K, def. 0.78, editable) y h_max (altura inicial). Temp es referencia.
    // Mensurandos agregados (Motor F): Reynolds max/min usan el primer/ultimo par de puntos,
    // Reynolds medio los promedia y M_teorico cierra con la formula de la cuaderneta.
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
    // Mensurandos agregados (Motor F): se crean en orden porque se encadenan (Re_medio usa
    // Re_max/Re_min; M_teorico usa Re_medio). Solo en el alta fresca, para no re-crearlos si el
    // docente los edita/borra luego. Reynolds max/min referencian el primer/ultimo par de puntos
    // (h_first/h_first2/t_first/t_first2 y h_last/h_last2/t_last/t_last2, alias del Motor F).
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
    // Auto-curación: re-aplica en cada boot para bases que hayan quedado a medio migrar.
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

/// Siembra Filtros (ver [`seed_definitions`]).
async fn seed_filtros(pool: &SqlitePool) -> anyhow::Result<()> {
    // Filtros — barrido en frecuencia de un circuito RLC. Por punto: f (frecuencia fijada por el
    // alumno), VRpp y Vgpp (tensiones pico a pico medidas), a y b (semiejes de la figura de
    // Lissajous). R, C1 y C2 se miden una vez con instrumento (no son dados); fpasaje_exp y
    // fbloqueo_exp tambien se miden una vez, con el osciloscopio (frecuencia de pasaje/bloqueo
    // observada). El unico dato dado por la catedra es L. Intermedias: omega=2*pi*f (rad/s),
    // razon=VRpp/Vgpp (adimensional), phi=asin(b/a) (rad). Dos curvas (Motor B): razon vs omega
    // (amplitud) y phi vs omega (desfasaje), ambas con eje x logaritmico. Mensurandos teoricos,
    // comparables contra la frecuencia experimental medida (is_final): fpasaje=1/(2*pi*sqrt(L*
    // (C1+C2))) y fbloqueo=1/(2*pi*sqrt(L*C2)). Topologia confirmada: C2||L en serie con C1 y R.
    //
    // Migración de forma: la práctica quedó sembrada originalmente con R/C1/C2 como `qty_given`
    // (valor ± U cargado a mano) cuando en realidad son medidos por el alumno con instrumento, y
    // sin fpasaje_exp/fbloqueo_exp. `seed_practice` no resiembra sobre una base ya presente, así
    // que se limpia y se resiembra con la forma final (no hay mediciones reales bajo la forma
    // vieja, la práctica es reciente).
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
            sqlx::query(
                "DELETE FROM submission_measurements WHERE quantity_id IN \
                 (SELECT id FROM practice_quantities WHERE practice_id = 'filtros')",
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM practice_quantities WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM practice_results WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM practice_curves WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM practice_intermediates WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
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
            // Topología: C2||L en serie con C1 y R.
            // Resonancia serie (pasaje): f = 1/(2π√(L(C1+C2)))
            // Resonancia paralelo del tanque (bloqueo): f = 1/(2π√(LC2))
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
    // Auto-curación: re-aplica el flag correcto en cada boot para bases que hayan quedado a
    // medio migrar (p. ej. la migración de forma de arriba ya corrió una vez, pero R/C1/C2 o
    // fpasaje/fbloqueo quedaron con el flag viejo por algún otro motivo).
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
    // Nombres corregidos después del alta (subíndice de los capacitores, sin sufijo de
    // instrumento en las frecuencias experimentales): se resincronizan en cada boot.
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

/// Siembra CA/RLC (ver [`seed_definitions`]).
pub(super) async fn seed_ca_rlc(pool: &SqlitePool) -> anyhow::Result<()> {
    // CA (RLC) — circuito serie con generador Vg y componentes R, C, L (R, C y Vg medidos con
    // instrumento; L dato de catedra). Por canal (R, C, L) se mide el voltaje pico a pico
    // (VRpp/VCpp/VLpp) y los semiejes de Lissajous (a_X/b_X, con el osciloscopio) para el
    // desfasaje experimental
    // phiX_exp = asin(b_X/a_X). El motor `estadistico` no encadena mensurandos entre si (cada
    // `ResultInput` se evalua solo contra las magnitudes), asi que cada formula de abajo va
    // expandida completa en terminos de R/C/L/Vg/f_trabajo — no hay `omega`/`Z` intermedios
    // reutilizables. Topologia RLC serie estandar; tensiones pico a pico y signos de fase
    // confirmados con el docente (no hay hoja real de esta practica, a diferencia de las otras
    // 6 de Fase 15, pero se validaron directamente).
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
            // Frecuencia de resonancia experimental: se mide con el osciloscopio (barriendo
            // f_trabajo hasta la condicion de resonancia), no se deriva de otras magnitudes.
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
            // a/b: semiejes de Lissajous medidos con el osciloscopio; phi_exp = asin(b/a). El
            // símbolo (a_R/b_R...) ya se muestra antes del nombre en el formulario
            // (SYMBOL_FIRST_QUANTITIES en constants.js), así que el nombre no repite la letra.
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

/// Corrige en bases ya sembradas (antes del 2026-08-07) los nombres/fórmulas de CA/RLC que
/// cambiaron de "Tension" a "Voltaje" y de radianes a grados en los desfasajes (`seed_ca_rlc` es
/// idempotente y no re-siembra una práctica que ya tiene magnitudes). Solo toca filas que todavía
/// están en el estado viejo (`quantity = 'tension'`, `unit = 'rad'`), así que no pisa ediciones
/// del docente sobre estos mismos campos. También borra `I_exp`: la corriente no se mide en el
/// circuito, solo se deriva (`I_teo`); no tiene FK entrante (no aparece en mediciones ni en
/// `submission_student_results`, que referencian por símbolo texto, no por id).
pub(super) async fn fix_ca_rlc_labels(pool: &SqlitePool) -> anyhow::Result<()> {
    // Gateado por result_missing (patrón del resto del archivo): una vez borrada, esta rama
    // deja de ejecutarse en cada boot.
    if !result_missing(pool, "ca-rlc", "I_exp").await? {
        sqlx::query(
            "DELETE FROM practice_results WHERE practice_id = 'ca-rlc' AND symbol = 'I_exp'",
        )
        .execute(pool)
        .await?;
    }

    // f_res_exp: magnitud nueva (se mide con el osciloscopio), no existia en la siembra original.
    // Va arriba de f_trabajo: se corre +1 la posicion de f_trabajo en adelante y se inserta ahi.
    if quantity_missing(pool, "ca-rlc", "f_res_exp").await? {
        let f_trabajo_pos: (i64,) = sqlx::query_as(
            "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_trabajo'",
        )
        .fetch_one(pool)
        .await?;
        sqlx::query(
            "UPDATE practice_quantities SET position = position + 1 \
             WHERE practice_id = 'ca-rlc' AND position >= ?1",
        )
        .bind(f_trabajo_pos.0)
        .execute(pool)
        .await?;
        let mut conn = pool.acquire().await?;
        insert_quantity(
            &mut conn,
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
    }

    // Reordena f_res_exp arriba de f_trabajo si quedo mal ubicada: una version anterior de esta
    // migracion la insertaba al final en vez de antes de f_trabajo.
    // ponytail: a diferencia de las otras ramas de esta funcion, esta no tiene un flag de "ya
    // migrado" que la desactive sola (comparar posiciones es la unica forma de saber si sigue
    // mal ordenada) — el SELECT corre en cada boot, pero es un self-join de 2 filas en una
    // tabla de ~15 filas por practica: costo despreciable. Toda `fix_ca_rlc_labels` es temporal
    // (ver doc de la funcion) y se borra junto con el resto en una limpieza futura.
    let stray_position: Option<(i64, i64)> = sqlx::query_as(
        "SELECT fre.position, ft.position \
         FROM practice_quantities fre, practice_quantities ft \
         WHERE fre.practice_id = 'ca-rlc' AND fre.symbol = 'f_res_exp' \
           AND ft.practice_id = 'ca-rlc' AND ft.symbol = 'f_trabajo' \
           AND fre.position > ft.position",
    )
    .fetch_optional(pool)
    .await?;
    if let Some((f_res_exp_pos, f_trabajo_pos)) = stray_position {
        sqlx::query(
            "UPDATE practice_quantities SET position = position + 1 \
             WHERE practice_id = 'ca-rlc' AND position >= ?1 AND position < ?2",
        )
        .bind(f_trabajo_pos)
        .bind(f_res_exp_pos)
        .execute(pool)
        .await?;
        sqlx::query(
            "UPDATE practice_quantities SET position = ?1 \
             WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
        )
        .bind(f_trabajo_pos)
        .execute(pool)
        .await?;
    }

    // Quita el "(a)"/"(b)" que una version anterior de esta migracion agrego al nombre: quedo
    // redundante en cuanto el símbolo se empezó a mostrar antes del nombre en el formulario.
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

    // Desfasajes: rad -> grados, misma formula que en seed_ca_rlc (ver alli el detalle de
    // omega/xl/xc/phi_teo). Se gatea en `unit = 'rad'` (marca de que todavia no se migro).
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
    Ok(())
}

/// Siembra las definiciones iniciales de las prácticas (idempotente por práctica, ver
/// [`seed_practice`]). Las magnitudes/fórmulas salen de las técnicas de trabajo de Física 103.
/// Cada práctica es independiente (no comparten estado entre sí); una función por práctica
/// mantiene cada una navegable por separado en vez de un único bloque de ~700 líneas.
pub async fn seed_definitions(pool: &SqlitePool) -> anyhow::Result<()> {
    seed_p1_estadistica(pool).await?;
    seed_p3_relajacion(pool).await?;
    seed_p2_cc(pool).await?;
    seed_p3_relajacion_desfasaje(pool).await?;
    seed_fluidos1(pool).await?;
    seed_viscosidad(pool).await?;
    seed_fluidos2(pool).await?;
    seed_filtros(pool).await?;
    seed_ca_rlc(pool).await?;
    fix_ca_rlc_labels(pool).await?;
    Ok(())
}
