use valuable::NamedValues;

#[derive(Debug)]
pub(crate) struct NamedValues_<'a>(pub NamedValues<'a>);

impl<'a> PartialEq for NamedValues_<'a> {
    fn eq(&self, other: &Self) -> bool {
        todo!();
    }
}

impl<'a> Eq for NamedValues_<'a> {}

impl<'a> Default for NamedValues_<'a> {
    fn default() -> Self {
        Self( NamedValues { fields: Default::default(), values: Default::default() })
    }
}

impl<'a> NamedValues_<'a> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}