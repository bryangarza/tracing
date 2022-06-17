#[derive(Default)]
pub struct Metric<'a, T> {
    pub name: &'a str,
    pub value: T,
}

impl<'a, T> Metric<'a, T> {
    pub fn new(name: &'a str, value: T) -> Self {
        Metric {
            name,
            value,
        }
    }
}