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

## Ajuste lineal

Gráfico **Δt vs (√h₁ − √h_i)**:
- Ordenada al origen (*)
- Pendiente (*)
- **M medio** (*) — de la pendiente, por propagación.
- **M teórico** = K + 4·(L_capilar/D_capilar)·(16/Re_medio), con K = 0.78.

## Reynolds (agregado, sin incertidumbre)

- Re_max = 2ρ·((h₁−h₂)/(t₂−t₁))·(R²_recip/(μ_agua·R_capilar))  — usa los **dos primeros** puntos.
- Re_min = 2ρ·((h_{n−1}−h_n)/(t_n−t_{n−1}))·(R²_recip/(μ_agua·R_capilar)) — usa los **dos últimos**.
- Re_medio = media(Re_max, Re_min).

> Nota de modelado: Re_max/Re_min y M_teórico **referencian puntos específicos** (primero/último)
> y agregados — no se expresan con los motores actuales (regresión + escalares + derivadas por
> punto). Ver decisión de siembra.
