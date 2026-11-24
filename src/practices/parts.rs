use super::{PracticeDefinition, PracticePart};

#[derive(Debug, Default, serde::Serialize)]
pub struct CompareOptions {
    pub results: std::collections::BTreeMap<String, Vec<String>>,
    pub parts: std::collections::BTreeMap<String, Vec<String>>,
    pub shared: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ClientDefinition {
    #[serde(flatten)]
    pub definition: PracticeDefinition,
    pub resolved: Resolved,
}

impl From<PracticeDefinition> for ClientDefinition {
    fn from(definition: PracticeDefinition) -> Self {
        let resolved = definition.resolved();
        ClientDefinition {
            definition,
            resolved,
        }
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Resolved {
    pub symbol_parts: std::collections::BTreeMap<String, String>,
    pub determinations: std::collections::BTreeMap<String, ScopedDeterminations>,
}

#[derive(Debug, serde::Serialize)]
pub struct ScopedDeterminations {
    pub scope: String,
    #[serde(flatten)]
    pub determinations: super::Determinations,
}

fn symbols(view: &PracticeDefinition) -> Vec<String> {
    view.quantities.iter().map(|q| q.symbol.clone()).collect()
}

fn comparable_in(views: &[PracticeDefinition], result: &str) -> Vec<String> {
    common_quantities(
        views
            .iter()
            .filter(|v| v.results.iter().any(|r| r.symbol == result)),
    )
}

fn common_quantities<'a>(views: impl Iterator<Item = &'a PracticeDefinition>) -> Vec<String> {
    views
        .map(symbols)
        .reduce(|mut all: Vec<String>, here: Vec<String>| {
            all.retain(|s| here.contains(s));
            all
        })
        .unwrap_or_default()
}

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
        self.result_part_in(symbol, &mut Vec::new())
    }

    fn result_part_in<'a>(&'a self, symbol: &str, seen: &mut Vec<String>) -> Option<&'a str> {
        if self
            .sections
            .iter()
            .any(|s| s.results.iter().any(|r| r == symbol))
        {
            return self.part_of(symbol, false);
        }
        let result = self.results.iter().find(|r| r.symbol == symbol)?;
        if seen.iter().any(|s| s == symbol) {
            return None;
        }
        seen.push(symbol.to_string());
        let part = self.inferred_part_in(&result.formula, seen);
        seen.pop();
        part.or_else(|| self.used_by_series(symbol))
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
        self.inferred_part_in(formula, &mut Vec::new())
    }

    fn inferred_part_in<'a>(&'a self, formula: &str, seen: &mut Vec<String>) -> Option<&'a str> {
        let mut parts: Vec<&str> = crate::computation::formula_symbols(formula)
            .iter()
            .filter_map(|s| self.symbol_part_in(s, seen))
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

    fn views(&self) -> Vec<PracticeDefinition> {
        if self.parts.is_empty() {
            return vec![self.clone()];
        }
        std::iter::once(self.shared_view())
            .chain(
                self.parts
                    .iter()
                    .filter(|p| self.part_has_content(p))
                    .map(|p| self.part_view(p)),
            )
            .collect()
    }

    pub fn comparable_quantities(&self, result: &str) -> Vec<String> {
        comparable_in(&self.views(), result)
    }

    pub fn compare_options(&self) -> CompareOptions {
        let views = self.views();
        CompareOptions {
            results: self
                .results
                .iter()
                .map(|r| (r.symbol.clone(), comparable_in(&views, &r.symbol)))
                .collect(),
            parts: self
                .parts
                .iter()
                .map(|p| (p.id.clone(), symbols(&self.part_view(p))))
                .collect(),
            shared: common_quantities(views.iter()),
        }
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

    /// Los mensurandos que se calculan en cada determinación, con las determinaciones de su parte.
    ///
    /// ```
    /// use quantify::practices::{Determinations, PracticeDefinition};
    /// let q = quantify::db::PracticeQuantity { symbol: "T".into(), per_determination: true, ..Default::default() };
    /// let r = |s: &str, f: &str| quantify::db::PracticeResult { symbol: s.into(), formula: f.into(), ..Default::default() };
    /// let d = Determinations { count: 3, label: "Medida".into(), average: true, display: Default::default() };
    /// let def = PracticeDefinition { quantities: vec![q], results: vec![r("g", "2*T"), r("k", "3")], determinations: Some(d), ..Default::default() };
    /// let found = def.determination_results();
    /// assert_eq!(found.len(), 1);
    /// assert_eq!((found[0].0.as_str(), found[0].1.count), ("g", 3));
    /// ```
    pub fn determination_results(&self) -> Vec<(String, super::Determinations)> {
        let views: Vec<PracticeDefinition> = if self.parts.is_empty() {
            vec![self.clone()]
        } else {
            self.parts.iter().map(|p| self.part_view(p)).collect()
        };
        views
            .into_iter()
            .filter(|v| !is_point_based(v.analysis_kind.as_deref()))
            .flat_map(|v| {
                let Some(d) = v.determinations.clone() else {
                    return Vec::new();
                };
                let per = crate::computation::per_determination_symbols(&v.quantities, &v.results);
                v.results
                    .iter()
                    .filter(|r| per.contains(&r.symbol))
                    .map(|r| (r.symbol.clone(), d.clone()))
                    .collect()
            })
            .collect()
    }

    pub fn averaged_results(&self) -> Vec<String> {
        self.determination_results()
            .into_iter()
            .filter(|(_, d)| d.average)
            .map(|(s, _)| s)
            .collect()
    }

    /// La parte de cada símbolo y las determinaciones de cada magnitud y mensurando que se repite.
    ///
    /// ```
    /// use quantify::practices::{Determinations, PracticeDefinition};
    /// let q = quantify::db::PracticeQuantity { symbol: "T".into(), per_determination: true, ..Default::default() };
    /// let r = |s: &str, f: &str| quantify::db::PracticeResult { symbol: s.into(), formula: f.into(), ..Default::default() };
    /// let d = Determinations { count: 3, label: "Medida".into(), average: true, display: Default::default() };
    /// let def = PracticeDefinition { quantities: vec![q], results: vec![r("g", "2*T"), r("k", "3")], determinations: Some(d), ..Default::default() };
    /// let resolved = def.resolved();
    /// assert_eq!(resolved.determinations["g"].determinations.count, 3);
    /// assert!(resolved.determinations.contains_key("T") && !resolved.determinations.contains_key("k"));
    /// ```
    pub fn resolved(&self) -> Resolved {
        let symbols = self
            .quantities
            .iter()
            .map(|q| q.symbol.as_str())
            .chain(self.results.iter().map(|r| r.symbol.as_str()))
            .chain(self.intermediates.iter().map(|i| i.symbol.as_str()))
            .chain(self.aggregates.iter().map(|a| a.symbol.as_str()))
            .chain(self.point_results.iter().map(|p| p.symbol.as_str()));
        let symbol_parts: std::collections::BTreeMap<String, String> = symbols
            .filter_map(|s| Some((s.to_string(), self.symbol_part(s)?.to_string())))
            .collect();
        let scoped = |symbol: &str, d: &super::Determinations| ScopedDeterminations {
            scope: symbol_parts.get(symbol).cloned().unwrap_or_default(),
            determinations: d.clone(),
        };
        let mut determinations: std::collections::BTreeMap<String, ScopedDeterminations> = self
            .quantities
            .iter()
            .filter_map(|q| {
                let part = self
                    .quantity_part(&q.symbol)
                    .and_then(|id| self.parts.iter().find(|p| p.id == id));
                let d = self.determinations_in(part)?;
                Some((q.symbol.clone(), scoped(&q.symbol, d)))
            })
            .collect();
        for (symbol, d) in self.determination_results() {
            let entry = scoped(&symbol, &d);
            determinations.insert(symbol, entry);
        }
        Resolved {
            symbol_parts,
            determinations,
        }
    }

    /// Las determinaciones que valen en una parte estadística, o en la práctica si no tiene partes.
    ///
    /// ```
    /// use quantify::practices::{Determinations, PracticeDefinition};
    /// let d = Determinations { count: 2, label: "Medida".into(), average: false, display: Default::default() };
    /// let def = PracticeDefinition { determinations: Some(d), ..Default::default() };
    /// assert_eq!(def.determinations_in(None).map(|d| d.count), Some(2));
    /// ```
    pub fn determinations_in<'a>(
        &'a self,
        part: Option<&'a PracticePart>,
    ) -> Option<&'a super::Determinations> {
        match part {
            Some(part) => part
                .determinations
                .as_ref()
                .filter(|_| self.part_kind(part) == "estadistico"),
            None if self.parts.is_empty() && !is_point_based(self.analysis_kind.as_deref()) => {
                self.determinations.as_ref()
            }
            None => None,
        }
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
            determinations: None,
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
            determinations: part
                .determinations
                .clone()
                .filter(|_| self.part_kind(part) == "estadistico"),
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
                .filter(|r| match self.result_part(&r.symbol) {
                    Some(p) => p == part.id,
                    None => self.purely_shared(&r.formula, &mut Vec::new()),
                })
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

    fn purely_shared(&self, formula: &str, seen: &mut Vec<String>) -> bool {
        crate::computation::formula_symbols(formula)
            .iter()
            .all(|s| {
                let Some(result) = self.results.iter().find(|r| r.symbol == *s) else {
                    return self.symbol_part(s).is_none();
                };
                if seen.contains(s) || self.result_part(s).is_some() {
                    return false;
                }
                seen.push(s.clone());
                let shared = self.purely_shared(&result.formula, seen);
                seen.pop();
                shared
            })
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
        self.symbol_part_in(symbol, &mut Vec::new())
    }

    fn symbol_part_in<'a>(&'a self, symbol: &str, seen: &mut Vec<String>) -> Option<&'a str> {
        if let Some(part) = self.quantity_part(symbol) {
            return Some(part);
        }
        if self.results.iter().any(|r| r.symbol == symbol) {
            return self.result_part_in(symbol, seen);
        }
        if let Some(base) = symbol
            .strip_suffix("_medio")
            .filter(|_| {
                let named = |s: &str| s == symbol;
                !self.quantities.iter().any(|q| named(&q.symbol))
                    && !self.intermediates.iter().any(|i| named(&i.symbol))
                    && !self.aggregates.iter().any(|a| named(&a.symbol))
                    && !self.point_results.iter().any(|p| named(&p.symbol))
            })
            .filter(|b| self.results.iter().any(|r| r.symbol == *b))
        {
            let part = self.result_part_in(base, seen);
            let averaged = self
                .determinations_in(part.and_then(|id| self.parts.iter().find(|p| p.id == id)))
                .is_some_and(|d| d.average)
                && crate::computation::per_determination_symbols(&self.quantities, &self.results)
                    .iter()
                    .any(|s| s == base);
            if averaged {
                return part;
            }
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
