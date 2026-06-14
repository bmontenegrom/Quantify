# Hoja de Resultados — Fluidos II

> Extraído del `.odt` original. Fuente: `tecnicas 103/hojas de resultados/Hoja de Resultados_Fluidos II.odt`.

## Datos (compartidos; (*) llevan incertidumbre)

- Radio del Capilar (m) (*)
- Longitud del Capilar (m) (*)
- Radio del Recipiente (m) (*)
- Temperatura (°C) — sin incertidumbre (sirve para la viscosidad del agua de tabla)
- g (m/s²) (*)
- ρ del agua — medida al final con densímetro (la viscosidad del agua sale de tabla según T)

## Medidas (≈12 puntos)

Por punto: Altura `h` (m) y Tiempo `t` (s). El reloj arranca en la altura máxima → **t₁ = 0**.
Columnas derivadas: Δt = t_i − t₁ (= t_i) y (√h₁ − √h_i), con h₁ = altura inicial (máxima).

## Ecuación de balance (reconstruida del PDF, confirmada por el docente)

```
t = 2 · (R²_recip / R²_capilar) · √((2 + M_medio) / (2g)) · (√h_inicial − √h_final)
```

Con `t₁ = 0` ⇒ `y = t` (tiempo crudo) y `x = √h_max − √h`. La pendiente del ajuste es
`2·(R²_recip/R²_cap)·√((2+M)/(2g))`, de donde:

```
M_medio = 2·g·(slope·R_cap² / (2·R_recip²))² − 2
```

## Reynolds y M teórico (agregados; referencian primer/último punto; sin incertidumbre)

```
Re_max = 2ρ · ((h₁−h₂)/(t₂−t₁)) · (R²_recip / (μ_agua·R_capilar))   (dos primeros puntos)
Re_min = 2ρ · ((h_{n−1}−h_n)/(t_n−t_{n−1})) · (R²_recip / (μ_agua·R_capilar))   (dos últimos)
Re_medio = (Re_max + Re_min) / 2
M_teórico = 0.78 + 4·(L_cap/(2·R_cap))·(16/Re_medio)
```

donde μ_agua (viscosidad del agua) sale de tabla según la temperatura (la carga el alumno).

> Nota de modelado: Re_max/Re_min y M_teórico **referencian puntos específicos** (primero/último)
> y agregan un escalar — se expresan con **Motor F** (mensurandos agregados, PR #38) usando los
> alias de extremo `h_first`/`h_first2`/`t_first`/`t_first2` (y `_last`/`_last2`). `h_max` es un
> escalar compartido porque las fórmulas de eje no pueden referenciar extremos.
>
> **Estado: las cuatro ecuaciones (balance/M_medio, Re_max/Re_min, Re_medio, M_teórico) fueron
> confirmadas por el docente (2026-06-13).**
