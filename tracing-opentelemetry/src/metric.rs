use std::fmt;
use tracing::field::Visit;
use tracing_core::Field;

#[derive(Default, Debug)]
pub(crate) struct Metric<T> {
    pub(crate) name: String,
    pub(crate) value: T,
}

pub(crate) struct MetricVisitor<'a>(pub(crate) &'a mut Metric<u64>);

impl<'a> Visit for MetricVisitor<'a> {
    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {
        // Do nothing
    }

    // fn record_str(&mut self, field: &Field, value: &str) {
    //     if field.name() == "metric.name" {
    //         self.0.name = value.to_string().into();
    //     }
    // }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name().starts_with("METRIC_") {
            self.0.name = field.name().to_string();
            self.0.value = value;
        }
    }
}
