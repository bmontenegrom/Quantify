# 02 — Referencia de dominio: Física 103

Extracto de la cuaderneta *Física 103 (DETEMA, Facultad de Química, UdelaR, 2022/2023)*
con lo necesario para modelar el sistema. Las citas resumen el texto; las fórmulas se
expresan en notación ASCII porque las originales están como imágenes en el `.docx`.

## Prácticas del curso

El curso consta de 9 prácticas agrupadas en 3 bloques:

| Nº | Práctica | Tipo de análisis dominante |
|----|----------|----------------------------|
| **1** | **Tratamiento Estadístico de Datos Experimentales** | Estadístico + incertidumbres + determinación indirecta |
| **2** | **Circuitos de Corriente Continua** | Medidas con amperímetro/voltímetro (escalas, resistencia interna) |
| **3** | **Relajación Exponencial** | Ajuste exponencial `V(t)=V₀·e^(−t/τ)` (linealizable) |
| 4 | Circuitos de Corriente Alterna | (futuro) |
| 5 | Circuito Filtro | (futuro) |
| 6 | Hidrostática y Tensión Superficial | (futuro — densímetro, balanza de Mohr) |
| 7 | Viscosidad | (futuro) |
| 8 | Fluidos I | (futuro) |
| 9 | Fluidos II | (futuro) |

**Alcance de esta iteración: P1, P2 y P3.**

## Evaluación del curso

> *Fuera de alcance de esta iteración, documentado para futura alineación del módulo de notas.*

- **Ganancia**: 100% de asistencia (máx. 3 faltas, todas recuperables) y ≥ **51/100** puntos.
- **Preguntas previas — 20 pts**: escritas en los primeros 15 min de clase.
- **Hojas de resultados — 30 pts**: **calificadas por mesa de trabajo**; misma nota a cada
  estudiante que firma la mesa. ⟵ implica que la entrega/corrección se asocia a una **mesa**, no solo a un estudiante.
- **Controles — 50 pts**: Control 1 = 20 pts (mínimo 4), Control 2 = 30 pts (mínimo 6).

El modelo actual de notas (`pregunta`/`informe`/`parcial` con sobre + valor normalizado)
es genérico y **no** refleja todavía este esquema ni la corrección por mesa.

## Modelo de incertidumbres (núcleo de "instrumentos y tipos de instrumentos")

### Incertidumbre tipo A (estadística)

Se obtiene repitiendo `n` medidas y calculando la **desviación estándar de la media**:

```
u_A = s_m = s / sqrt(n)
```

donde `s` es la desviación estándar muestral de las `n` medidas.
⚠️ CONFIRMAR: la cuaderneta usa `s` muestral (`/(n−1)`). El `analysis.rs` actual usa poblacional (`/n`); hay que cambiarlo para tipo A.

### Incertidumbre tipo B (no estadística, proviene del instrumento)

Es "cualquier incertidumbre que no sea tipo A". Sus componentes más comunes derivan del
instrumento de medida, y **el tipo de instrumento determina la fórmula**:

- **Instrumento digital → error de truncamiento/resolución.**
  `R` = resolución = menor indicación que puede dar el instrumento.
  ```
  u_B = R / (2*sqrt(3))      # distribución rectangular de ancho R
  ```
- **Instrumento analógico → error de estimación/apreciación.**
  `A` = apreciación = fracción más pequeña en que el operador puede dividir la indicación
  (la "indicación" es la menor distancia entre dos marcas consecutivas de la escala).
  ```
  u_B = A / sqrt(6)        # distribución triangular (confirmado por el docente)
  ```

- **Tester y osciloscopio → incertidumbre del fabricante (modelo de tres términos).**
  No se modelan con la resolución simple. El fabricante/la técnica especifica, **por escala**,
  la **incertidumbre expandida U (k=2)** como combinación de un término proporcional al valor
  leído, un término proporcional al paso de escala y un término fijo:
  ```
  U_spec = (pct_lectura/100)*|valor| + coef*step + fijo     # tal como figura en la hoja/técnica
  u_B    = U_spec / 2                                        # k=2 (confirmado por el docente)
  ```
  Depende del **valor medido**, no solo de la resolución. Casos reales:
  - **Tester A830L (corriente)** — `pct=1.0`, `coef=5` (5 dgt), `fijo=0`, `step`=resolución:
    200 µA–20 mA → `±(1.0% + 5 dgt)`; 200 mA → `±(2.0% + 5 dgt)`. Cada escala tiene resistencia
    interna con su propia incertidumbre, p. ej. `(1002 ± 10) Ω`.
  - **Tester EXTECH MN35** — voltaje `±(0.5% + 2 dgt)`; resistencia `±(0.8% + 4 dgt)` y `±(0.8% + 2 dgt)`.
  - **Osciloscopio GW Instek GDS-1052-U (voltaje, eje Y)** — `pct=3.0`, `coef=0.1`
    (`step`=VOLTS/DIV), `fijo=1 mV`:
    ```
    U_Y,TOTAL = ± [ 3% del valor medido + 0.1*(VOLTS/DIV) + 1 mV ]   (Técnica de Trabajo 2022)
    ```
    Para el osciloscopio la **incertidumbre tipo A es despreciable** frente a la tipo B (solo
    se considera tipo B). Con `u_B = U_Y,TOTAL/2` y tipo A ≈ 0, al re-expandir (×2) se recupera
    exactamente `U_Y,TOTAL` ⇒ el modelo es autoconsistente con la "incertidumbre total" del PDF.
    El desfasaje `φ = arcsen(b/a)` se obtiene por **propagación de varianzas** desde `a` y `b`
    (ambas medidas con la misma fórmula, en la escala usada para la elipse).
    ⚠️ PENDIENTE: la incertidumbre del **eje X (tiempo)** del osciloscopio para P3 (tiempo de
    descarga por CURSOR) — fórmula análoga sobre TIME/DIV, a confirmar con la técnica de P3.

> **Modelos confirmados por el docente**: analógico `A/√6` (triangular), digital simple
> `R/(2√3)` (rectangular), y `fabricante` (tester y osciloscopio) con
> `u_B = U_spec/2`, `U_spec = pct·|valor| + coef·step + fijo` (k=2).

Otras componentes tipo B mencionadas (a considerar a futuro, no en el cálculo base):
falta de linealidad de la escala, deriva temporal, ajuste inadecuado del cero, y el error
de modelo en determinaciones indirectas (fórmulas aproximadas).

### Incertidumbre combinada y expandida

Para una magnitud medida directamente, se combinan ambas componentes:

```
u_c = sqrt(u_A^2 + u_B^2)
```

El resultado se expresa con la **incertidumbre expandida** (95% de confianza, convención del curso):

```
U = k * u_c,   con k = 2   ->   U = 2 * u_c
```

### Determinaciones indirectas y propagación de varianzas

Si el mensurando se calcula como `Q = f(a, b, c, ...)` a partir de variables medidas
`a, b, c`, con incertidumbres `u_a, u_b, u_c`:

```
u_Q^2 = (∂f/∂a)^2 · u_a^2 + (∂f/∂b)^2 · u_b^2 + (∂f/∂c)^2 · u_c^2 + ...
U_Q   = 2 * u_Q        # 95%
```

Las derivadas parciales se evalúan en los valores medios. Ejemplo de la cuaderneta:
área de un cordón `Q = f(l,a,b) = l·a + l·b`.

> **Implicación de diseño**: el motor necesita evaluar `f` y sus derivadas. Ver
> [`03-modelo-datos-y-motor.md`](03-modelo-datos-y-motor.md#propagación) para la opción
> recomendada (propagación numérica por diferencias finitas con un evaluador de expresiones).

## Tipos de instrumento y ejemplos (catálogo)

El "tipo de instrumento" fundamental para el cálculo es **analógico** vs **digital**.
Instrumentos concretos citados en la cuaderneta (insumo para el catálogo inicial):

| Instrumento | Tipo típico | Magnitud | Notas |
|-------------|-------------|----------|-------|
| Regla / cinta | analógico | longitud | apreciación según menor división |
| Calibre (Vernier) | analógico | longitud | apreciación fina |
| Balanza | analógico/digital | masa | |
| Cronómetro | digital | tiempo | resolución del visor |
| Amperímetro | analógico/digital | corriente | **escalas**; resistencia interna; "ohm/V" |
| Voltímetro | analógico/digital | voltaje | **escalas**; resistencia interna |
| Densímetro | analógico | densidad | (P6) |
| Balanza de Mohr | analógico | densidad/empuje | (P6) |
| Osciloscopio | analógico | voltaje/tiempo | (P4/P5) |

Para P2 (corriente continua), los instrumentos tienen **varias escalas** y cada escala
tiene su propia resolución/apreciación y resistencia interna; el estudiante elige la
escala más adecuada. El modelo de datos contempla escalas por instrumento.
