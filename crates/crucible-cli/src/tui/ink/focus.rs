use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FocusId(pub String);

impl FocusId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for FocusId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for FocusId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Default)]
pub struct FocusContext {
    active: Option<FocusId>,
    order: Vec<FocusId>,
    registered: HashSet<FocusId>,
    auto_focus_pending: Option<FocusId>,
}

impl FocusContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: FocusId, auto_focus: bool) {
        if !self.registered.contains(&id) {
            self.registered.insert(id.clone());
            self.order.push(id.clone());

            if auto_focus && self.active.is_none() && self.auto_focus_pending.is_none() {
                self.auto_focus_pending = Some(id);
            }
        }
    }

    pub fn unregister(&mut self, id: &FocusId) {
        self.registered.remove(id);
        self.order.retain(|i| i != id);

        if self.active.as_ref() == Some(id) {
            self.active = self.order.first().cloned();
        }
    }

    pub fn clear_registrations(&mut self) {
        self.order.clear();
        self.registered.clear();
        self.active = None;
    }

    pub fn apply_auto_focus(&mut self) {
        if let Some(id) = self.auto_focus_pending.take() {
            if self.active.is_none() {
                self.active = Some(id);
            }
        }
    }

    pub fn focus_next(&mut self) {
        if self.order.is_empty() {
            return;
        }

        match &self.active {
            Some(current) => {
                if let Some(idx) = self.order.iter().position(|i| i == current) {
                    let next_idx = (idx + 1) % self.order.len();
                    self.active = Some(self.order[next_idx].clone());
                }
            }
            None => {
                self.active = self.order.first().cloned();
            }
        }
    }

    pub fn focus_prev(&mut self) {
        if self.order.is_empty() {
            return;
        }

        match &self.active {
            Some(current) => {
                if let Some(idx) = self.order.iter().position(|i| i == current) {
                    let prev_idx = if idx == 0 {
                        self.order.len() - 1
                    } else {
                        idx - 1
                    };
                    self.active = Some(self.order[prev_idx].clone());
                }
            }
            None => {
                self.active = self.order.last().cloned();
            }
        }
    }

    pub fn focus(&mut self, id: impl Into<FocusId>) {
        let id = id.into();
        if self.registered.contains(&id) {
            self.active = Some(id);
        }
    }

    pub fn blur(&mut self) {
        self.active = None;
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.active.as_ref().is_some_and(|a| a.0 == id)
    }

    pub fn active_id(&self) -> Option<&FocusId> {
        self.active.as_ref()
    }

    pub fn focus_order(&self) -> &[FocusId] {
        &self.order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_next_cycles_through_elements() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("a"), false);
        ctx.register(FocusId::new("b"), false);
        ctx.register(FocusId::new("c"), false);

        assert!(ctx.active_id().is_none());

        ctx.focus_next();
        assert!(ctx.is_focused("a"));

        ctx.focus_next();
        assert!(ctx.is_focused("b"));

        ctx.focus_next();
        assert!(ctx.is_focused("c"));

        ctx.focus_next();
        assert!(ctx.is_focused("a"));
    }

    #[test]
    fn focus_prev_cycles_backwards() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("a"), false);
        ctx.register(FocusId::new("b"), false);
        ctx.register(FocusId::new("c"), false);

        ctx.focus("c");
        assert!(ctx.is_focused("c"));

        ctx.focus_prev();
        assert!(ctx.is_focused("b"));

        ctx.focus_prev();
        assert!(ctx.is_focused("a"));

        ctx.focus_prev();
        assert!(ctx.is_focused("c"));
    }

    #[test]
    fn auto_focus_sets_initial_focus() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("first"), true);
        ctx.register(FocusId::new("second"), false);

        ctx.apply_auto_focus();
        assert!(ctx.is_focused("first"));
    }

    #[test]
    fn auto_focus_only_applies_once() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("first"), true);
        ctx.apply_auto_focus();

        ctx.register(FocusId::new("second"), false);
        ctx.focus("second");
        ctx.register(FocusId::new("third"), true);
        ctx.apply_auto_focus();

        assert!(ctx.is_focused("second"));
    }

    #[test]
    fn unregister_removes_element() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("a"), false);
        ctx.register(FocusId::new("b"), false);
        ctx.focus("a");

        ctx.unregister(&FocusId::new("a"));

        assert_eq!(ctx.focus_order().len(), 1);
        assert!(ctx.is_focused("b"));
    }

    #[test]
    fn focus_invalid_id_is_ignored() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("valid"), false);
        ctx.focus("invalid");

        assert!(ctx.active_id().is_none());
    }

    #[test]
    fn duplicate_registration_is_ignored() {
        let mut ctx = FocusContext::new();
        ctx.register(FocusId::new("a"), false);
        ctx.register(FocusId::new("a"), false);

        assert_eq!(ctx.focus_order().len(), 1);
    }
}
