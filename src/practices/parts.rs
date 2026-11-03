use super::{PracticeDefinition, PracticePart};

pub fn is_point_based(kind: Option<&str>) -> bool {
    matches!(kind, Some("regresion_lineal" | "curva"))
}

impl PracticePart {
    pub fn kind(&self) -> &str {
        self.analysis_kind.as_deref().unwrap_or("estadistico")
    }
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
        let mut parts: Vec<&str> = crate::computation::formula_symbols(&result.formula)
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

    pub fn shared_view(&self) -> PracticeDefinition {
        PracticeDefinition {
            analysis_kind: Some("estadistico".into()),
            x_formula: None,
            y_formula: None,
            quantities: self
                .quantities
                .iter()
                .filter(|q| self.quantity_part(&q.symbol).is_none())
                .filter(|q| !q.per_point || q.is_given || !self.has_point_part())
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
        let point_based = is_point_based(Some(part.kind()));
        PracticeDefinition {
            analysis_kind: Some(part.kind().to_string()),
            x_formula: part.x_formula.clone(),
            y_formula: part.y_formula.clone(),
            operator_count: part.operator_count,
            quantities: self
                .quantities
                .iter()
                .filter(|q| match self.quantity_part(&q.symbol) {
                    Some(p) => p == part.id,
                    None => !q.per_point || q.is_given || point_based || !self.has_point_part(),
                })
                .cloned()
                .collect(),
            results: self
                .results
                .iter()
                .filter(|r| self.result_part(&r.symbol) == Some(part.id.as_str()))
                .cloned()
                .collect(),
            curves: if point_based {
                self.curves.clone()
            } else {
                vec![]
            },
            intermediates: if point_based {
                self.intermediates.clone()
            } else {
                vec![]
            },
            point_results: if point_based {
                self.point_results.clone()
            } else {
                vec![]
            },
            aggregates: if point_based {
                self.aggregates.clone()
            } else {
                vec![]
            },
            parts: vec![],
            sections: vec![],
            ..self.clone()
        }
    }

    pub fn point_view(&self) -> PracticeDefinition {
        if self.parts.is_empty() {
            return self.clone();
        }
        match self.parts.iter().find(|p| is_point_based(Some(p.kind()))) {
            Some(part) => self.part_view(part),
            None => self.shared_view(),
        }
    }

    pub fn symbol_part(&self, symbol: &str) -> Option<&str> {
        if let Some(part) = self.quantity_part(symbol) {
            return Some(part);
        }
        let point_part = self.parts.iter().find(|p| is_point_based(Some(p.kind())))?;
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
        from_series.then_some(point_part.id.as_str())
    }

    pub fn part_has_content(&self, part: &PracticePart) -> bool {
        self.quantities
            .iter()
            .any(|q| self.quantity_part(&q.symbol) == Some(part.id.as_str()))
            || self
                .results
                .iter()
                .any(|r| self.result_part(&r.symbol) == Some(part.id.as_str()))
            || (is_point_based(Some(part.kind()))
                && (!self.curves.is_empty()
                    || self.quantities.iter().any(|q| q.per_point && !q.is_given)))
    }

    pub fn series_parts(&self) -> Vec<String> {
        let mut parts: Vec<&super::PracticePart> = self
            .quantities
            .iter()
            .filter(|q| q.per_point && !q.is_given)
            .filter_map(|q| self.quantity_part(&q.symbol))
            .filter_map(|id| self.parts.iter().find(|p| p.id == id))
            .collect();
        parts.sort_by_key(|p| (p.position, p.id.clone()));
        parts.dedup_by(|a, b| a.id == b.id);
        parts.into_iter().map(|p| p.id.clone()).collect()
    }

    pub(super) fn resolve_part_kinds(&mut self) {
        if self.parts.iter().all(|p| p.analysis_kind.is_some()) {
            return;
        }
        let series = if is_point_based(self.analysis_kind.as_deref()) {
            self.series_parts()
                .into_iter()
                .next()
                .or_else(|| self.parts.first().map(|p| p.id.clone()))
        } else {
            None
        };
        for part in &mut self.parts {
            if part.analysis_kind.is_some() {
                continue;
            }
            part.inherited = true;
            if series.as_deref() == Some(part.id.as_str()) {
                part.analysis_kind = self.analysis_kind.clone();
                part.x_formula = self.x_formula.clone();
                part.y_formula = self.y_formula.clone();
            } else {
                part.analysis_kind = Some("estadistico".into());
                part.operator_count = self.operator_count;
            }
        }
    }

    pub fn reresolved(&self) -> PracticeDefinition {
        let mut definition = self.clone();
        for part in definition.parts.iter_mut().filter(|p| p.inherited) {
            part.analysis_kind = None;
            part.x_formula = None;
            part.y_formula = None;
            part.operator_count = None;
            part.inherited = false;
        }
        definition.resolve_part_kinds();
        definition
    }

    fn has_point_part(&self) -> bool {
        self.parts.iter().any(|p| is_point_based(Some(p.kind())))
    }
}
