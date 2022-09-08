#![allow(missing_docs)]
use valuable::{NamedValues, NamedField, Value};

use crate::valuable::NamedValues_;

use super::{metadata, span, Parent};

use std::fmt;

/// A mock event.
///
/// This is intended for use with the mock subscriber API in the
/// `subscriber` module.
#[derive(Default, Eq, PartialEq)]
pub struct MockEvent<'a> {
    pub fields: Option<NamedValues_<'a>>,
    pub(crate) parent: Option<Parent>,
    in_spans: Vec<span::MockSpan>,
    metadata: metadata::Expect,
}

pub fn mock<'a>() -> MockEvent<'a> {
    MockEvent {
        ..Default::default()
    }
}

pub fn msg<'a>(message: impl fmt::Display) -> MockEvent<'a> {
    let fields = [
        NamedField::new("message"),
    ];
    let values = [
        Value::String(message.to_string().as_str()),
    ];

    let named_values = NamedValues::new(&fields, &values);
    mock().with_fields(NamedValues_(named_values))
}

impl<'a> MockEvent<'a> {
    pub fn named<I>(self, name: I) -> Self
    where
        I: Into<String>,
    {
        Self {
            metadata: metadata::Expect {
                name: Some(name.into()),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn with_fields(self, fields: NamedValues_<'a>) -> Self
    {
        Self {
            fields: Some(fields),
            ..self
        }
    }

    pub fn at_level(self, level: tracing::Level) -> Self {
        Self {
            metadata: metadata::Expect {
                level: Some(level),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn with_target<I>(self, target: I) -> Self
    where
        I: Into<String>,
    {
        Self {
            metadata: metadata::Expect {
                target: Some(target.into()),
                ..self.metadata
            },
            ..self
        }
    }

    pub fn with_explicit_parent(self, parent: Option<&str>) -> MockEvent {
        let parent = match parent {
            Some(name) => Parent::Explicit(name.into()),
            None => Parent::ExplicitRoot,
        };
        Self {
            parent: Some(parent),
            ..self
        }
    }

    pub fn check(
        &mut self,
        event: &tracing::Event<'_>,
        get_parent_name: impl FnOnce() -> Option<String>,
        collector_name: &str,
    ) {
        let meta = event.metadata();
        let name = meta.name();
        self.metadata
            .check(meta, format_args!("event \"{}\"", name), collector_name);
        assert!(
            meta.is_event(),
            "[{}] expected {}, but got {:?}",
            collector_name,
            self,
            event
        );
        if let Some(expected_fields) = self.fields {
            let named_values = NamedValues_(*event.fields());
            assert_eq!(expected_fields, named_values);
        }

        if let Some(ref expected_parent) = self.parent {
            let actual_parent = get_parent_name();
            expected_parent.check_parent_name(
                actual_parent.as_deref(),
                event.parent().cloned(),
                event.metadata().name(),
                collector_name,
            )
        }
    }

    pub fn in_scope(self, spans: impl IntoIterator<Item = span::MockSpan>) -> Self {
        Self {
            in_spans: spans.into_iter().collect(),
            ..self
        }
    }

    pub fn scope_mut(&mut self) -> &mut [span::MockSpan] {
        &mut self.in_spans[..]
    }
}

impl<'a> fmt::Display for MockEvent<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "an event{}", self.metadata)
    }
}

impl<'a> fmt::Debug for MockEvent<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("MockEvent");

        if let Some(ref name) = self.metadata.name {
            s.field("name", name);
        }

        if let Some(ref target) = self.metadata.target {
            s.field("target", target);
        }

        if let Some(ref level) = self.metadata.level {
            s.field("level", &format_args!("{:?}", level));
        }

        if let Some(ref fields) = self.fields {
            s.field("fields", fields);
        }

        if let Some(ref parent) = self.parent {
            s.field("parent", &format_args!("{:?}", parent));
        }

        if !self.in_spans.is_empty() {
            s.field("in_spans", &self.in_spans);
        }

        s.finish()
    }
}
