use super::{PracticeDefinition, PracticePart};

pub fn is_point_based(kind: Option<&str>) -> bool {
    matches!(kind, Some("regresion_lineal" | "curva"))
}

impl PracticeDefinition {
    fn part_of(&self, symbol: &str, quantity: bool) -> Option<&str> {
        self.sections
            .iter()
            .find(|s| {
                let members = if quantity { &s.quantities } else { &s.results };
                members.iter().any(|m| m == symbol)
            })
            .and_then(|s| s.part_id.as_deref())
            .filter(|p| self.parts.iter().any(|part| part.id == *p))
    }

    pub fn quantity_part(&self, symbol: &str) -> Option<&str> {
        self.part_of(symbol, true)
    }

    pub fn result_part(&self, symbol: &str) -> Option<&str> {
        if self
            .sections
            .iter()
            .any(|s| s.results.iter().any(|r| r == symbol))
        {
            return self.part_of(symbol, false);
        }
        let result = self.results.iter().find(|r| r.symbol == symbol)?;
        self.inferred_part(&result.formula)
            .or_else(|| self.used_by_series(symbol))
    }

    fn used_by_series(&self, symbol: &str) -> Option<&str> {
        let series = self.series_part()?;
        let formulas = self
            .point_results
            .iter()
            .map(|p| p.formula.as_str())
            .chain(self.aggregates.iter().map(|a| a.formula.as_str()));
        formulas
            .flat_map(crate::computation::formula_symbols)
            .any(|s| s == symbol)
            .then_some(series.id.as_str())
    }

    pub fn inferred_part(&self, formula: &str) -> Option<&str> {
        let mut parts: Vec<&str> = crate::computation::formula_symbols(formula)
            .iter()
            .filter_map(|s| self.symbol_part(s))
            .collect();
        parts.sort();
        parts.dedup();
        match parts.as_slice() {
            [part] => Some(part),
            _ => None,
        }
    }

    pub fn series_parts(&self) -> Vec<String> {
        if !is_point_based(self.analysis_kind.as_deref()) {
            return vec![];
        }
        self.parts
            .iter()
            .filter(|p| {
                self.quantities.iter().any(|q| {
                    q.per_point && !q.is_given && self.quantity_part(&q.symbol) == Some(&p.id)
                })
            })
            .map(|p| p.id.clone())
            .collect()
    }

    pub fn series_part(&self) -> Option<&PracticePart> {
        if !is_point_based(self.analysis_kind.as_deref()) {
            return None;
        }
        if let Some(chosen) = self
            .parts
            .iter()
            .find(|p| is_point_based(p.analysis_kind.as_deref()))
        {
            return Some(chosen);
        }
        if self.parts.iter().any(|p| p.analysis_kind.is_some()) {
            return None;
        }
        let holding = self.series_parts();
        self.parts
            .iter()
            .find(|p| holding.contains(&p.id))
            .or_else(|| self.parts.first())
    }

    pub fn stray_series(&self) -> Option<&PracticePart> {
        let series = self.series_part()?;
        self.series_parts()
            .iter()
            .any(|p| *p != series.id)
            .then_some(series)
    }

    pub fn part_kinds(&self) -> Vec<String> {
        let series = self.series_part().map(|p| p.id.as_str());
        self.parts
            .iter()
            .map(|p| self.kind_given_series(p, series).to_string())
            .collect()
    }

    pub fn with_effective_kinds(mut self) -> PracticeDefinition {
        let kinds = self.part_kinds();
        for (part, kind) in self.parts.iter_mut().zip(kinds) {
            part.effective_kind = kind;
        }
        self
    }

    pub fn with_mirrored_kind(mut self) -> PracticeDefinition {
        if self.parts.iter().any(|p| p.analysis_kind.is_some()) {
            self.analysis_kind = Some(
                self.parts
                    .iter()
                    .filter_map(|p| p.analysis_kind.clone())
                    .find(|k| is_point_based(Some(k)))
                    .unwrap_or_else(|| "estadistico".into()),
            );
        }
        self
    }

    pub fn part_kind(&self, part: &PracticePart) -> &str {
        self.kind_given_series(part, self.series_part().map(|p| p.id.as_str()))
    }

    fn kind_given_series(&self, part: &PracticePart, series: Option<&str>) -> &str {
        if series == Some(part.id.as_str()) {
            self.analysis_kind.as_deref().unwrap_or("estadistico")
        } else {
            "estadistico"
        }
    }

    fn statistical_operators(&self) -> Option<i64> {
        if is_point_based(self.analysis_kind.as_deref()) {
            None
        } else {
            self.operator_count
        }
    }

    pub fn shared_view(&self) -> PracticeDefinition {
        let has_series = self.series_part().is_some();
        PracticeDefinition {
            analysis_kind: Some("estadistico".into()),
            x_formula: None,
            y_formula: None,
            operator_count: self.statistical_operators(),
            quantities: self
                .quantities
                .iter()
                .filter(|q| self.quantity_part(&q.symbol).is_none())
                .filter(|q| !q.per_point || q.is_given || !has_series)
                .cloned()
                .collect(),
            results: self
                .results
                .iter()
                .filter(|r| self.result_part(&r.symbol).is_none())
                .cloned()
                .collect(),
            curves: vec![],
            intermediates: vec![],
            point_results: vec![],
            aggregates: vec![],
            parts: vec![],
            sections: vec![],
            ..self.clone()
        }
    }

    pub fn part_view(&self, part: &PracticePart) -> PracticeDefinition {
        let series = self.series_part().map(|p| p.id.as_str());
        let is_series = series == Some(part.id.as_str());
        PracticeDefinition {
            analysis_kind: Some(self.part_kind(part).to_string()),
            x_formula: self.x_formula.clone().filter(|_| is_series),
            y_formula: self.y_formula.clone().filter(|_| is_series),
            operator_count: self.statistical_operators(),
            quantities: self
                .quantities
                .iter()
                .filter(|q| match self.quantity_part(&q.symbol) {
                    Some(p) => p == part.id,
                    None => !q.per_point || q.is_given || is_series || series.is_none(),
                })
                .cloned()
                .collect(),
            results: self
                .results
                .iter()
                .filter(|r| self.result_part(&r.symbol) == Some(part.id.as_str()))
                .cloned()
                .collect(),
            curves: self.curves.iter().filter(|_| is_series).cloned().collect(),
            intermediates: self
                .intermediates
                .iter()
                .filter(|_| is_series)
                .cloned()
                .collect(),
            point_results: self
                .point_results
                .iter()
                .filter(|_| is_series)
                .cloned()
                .collect(),
            aggregates: self
                .aggregates
                .iter()
                .filter(|_| is_series)
                .cloned()
                .collect(),
            parts: vec![],
            sections: vec![],
            ..self.clone()
        }
    }

    pub fn point_view(&self) -> PracticeDefinition {
        if self.parts.is_empty() {
            return self.clone();
        }
        match self.series_part() {
            Some(part) => self.part_view(part),
            None => self.shared_view(),
        }
    }

    pub fn symbol_part(&self, symbol: &str) -> Option<&str> {
        if let Some(part) = self.quantity_part(symbol) {
            return Some(part);
        }
        let series_part = self.series_part()?;
        let series: Vec<&str> = self
            .quantities
            .iter()
            .filter(|q| q.per_point && !q.is_given)
            .map(|q| q.symbol.as_str())
            .chain(self.intermediates.iter().map(|it| it.symbol.as_str()))
            .collect();
        let from_series = ["slope", "intercept"].contains(&symbol)
            || series.contains(&symbol)
            || series.iter().any(|s| {
                symbol == format!("{s}_max")
                    || series.iter().any(|t| symbol == format!("{t}_at_{s}_max"))
            });
        from_series.then_some(series_part.id.as_str())
    }

    pub fn part_has_content(&self, part: &PracticePart) -> bool {
        self.quantities
            .iter()
            .any(|q| self.quantity_part(&q.symbol) == Some(part.id.as_str()))
            || self
                .results
                .iter()
                .any(|r| self.result_part(&r.symbol) == Some(part.id.as_str()))
            || (self.series_part().is_some_and(|s| s.id == part.id)
                && (!self.curves.is_empty()
                    || self.quantities.iter().any(|q| q.per_point && !q.is_given)))
    }
}
