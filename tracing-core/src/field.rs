//! Span and `Event` key-value data.
//!
//! Spans and events may be annotated with key-value data, referred to as known
//! as _fields_. These fields consist of a mapping from a key (corresponding to
//! a `&str` but represented internally as an array index) to a [`Value`].
//!
//! # `Value`s and `Collect`s
//!
//! Collectors consume `Value`s as fields attached to [span]s or [`Event`]s.
//! The set of field keys on a given span or is defined on its [`Metadata`].
//! When a span is created, it provides [`Attributes`] to the collector's
//! [`new_span`] method, containing any fields whose values were provided when
//! the span was created; and may call the collector's [`record`] method
//! with additional [`Record`]s if values are added for more of its fields.
//! Similarly, the [`Event`] type passed to the collector's [`event`] method
//! will contain any fields attached to each event.
//!
//! `tracing` represents values as either one of a set of Rust primitives
//! (`i64`, `u64`, `f64`, `i128`, `u128`, `bool`, and `&str`) or using a
//! `fmt::Display` or `fmt::Debug` implementation. Collectors are provided
//! these primitive value types as `dyn Value` trait objects.
//!
//! These trait objects can be formatted using `fmt::Debug`, but may also be
//! recorded as typed data by calling the [`Value::record`] method on these
//! trait objects with a _visitor_ implementing the [`Visit`] trait. This trait
//! represents the behavior used to record values of various types. For example,
//! we might record integers by incrementing counters for their field names,
//! rather than printing them.
//!
//! [span]: super::span
//! [`Event`]: super::event::Event
//! [`Metadata`]: super::metadata::Metadata
//! [`Attributes`]:  super::span::Attributes
//! [`Record`]: super::span::Record
//! [`new_span`]: super::collect::Collect::new_span
//! [`record`]: super::collect::Collect::record
//! [`event`]:  super::collect::Collect::event
use valuable::{Fields, NamedField, NamedValues, StructDef, Structable, Value, Visit};

use crate::callsite;
use core::fmt;

/// An empty field.
///
/// This can be used to indicate that the value of a field is not currently
/// present but will be recorded later.
///
/// When a field's value is `Empty`. it will not be recorded.
#[derive(Debug, Eq, PartialEq)]
pub struct Empty;

/// A set of fields and values for a span.
#[derive(Debug)]
pub struct ValueSet<'a> {
    // Note: public only temporarily (we may get rid of ValueSet altogether)
    pub values: &'a dyn Structable,
    pub callsite: callsite::Identifier,
}

// ===== impl ValueSet =====

impl<'a> ValueSet<'a> {
    /// Returns an [`Identifier`] that uniquely identifies the [`Callsite`]
    /// defining the fields this `ValueSet` refers to.
    ///
    /// [`Identifier`]: super::callsite::Identifier
    /// [`Callsite`]: super::callsite::Callsite
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.callsite.clone()
    }

    /// Visits all the fields in this `ValueSet` with the provided [visitor].
    ///
    /// [visitor]: Visit
    pub fn visit(&self, visitor: &mut dyn Visit) {
        self.values.visit(visitor);
    }

    /// Returns `true` if the top level of this `ValueSet` contains a value for
    /// the given `Field`.
    pub(crate) fn contains(&self, search_field: &NamedField<'_>) -> bool {
        if let StructDef::Static { fields, .. } = self.values.definition() {
            if let Fields::Named(named_fields) = fields {
                for named_field in named_fields.into_iter() {
                    if named_field.name() == search_field.name() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Returns true if this `ValueSet` contains _no_ values.
    pub(crate) fn is_empty(&self) -> bool {
        let mut visitor = IsEmptyVisitor { res: true };
        self.values.visit(&mut visitor);
        visitor.res
    }
}

struct IsEmptyVisitor {
    res: bool,
}

impl Visit for IsEmptyVisitor {
    fn visit_named_fields(&mut self, named_values: &NamedValues<'_>) {
        self.res = named_values.is_empty()
    }

    fn visit_value(&mut self, value: Value<'_>) {
        match value {
            Value::Structable(v) => v.visit(self),
            _ => {} // do nothing for other types
        }
    }
}

impl<'a> fmt::Display for ValueSet<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "callsite: {:?}, values: {:?}",
            self.callsite(),
            self.values
        )
    }
}

#[cfg(test)]
mod test {
    use valuable::Valuable;

    use super::*;
    use crate::metadata::{Kind, Level, Metadata};

    struct TestCallsite1;
    static TEST_CALLSITE_1: TestCallsite1 = TestCallsite1;
    static TEST_META_1: Metadata<'static> = metadata! {
        name: "field_test1",
        target: module_path!(),
        level: Level::INFO,
        fields: &[NamedField::new("foo"), NamedField::new("bar"), NamedField::new("baz")],
        callsite: callsite::Identifier(&TEST_CALLSITE_1),
        kind: Kind::SPAN,
    };

    impl crate::callsite::Callsite for TestCallsite1 {
        fn set_interest(&self, _: crate::collect::Interest) {
            unimplemented!()
        }

        fn metadata(&self) -> &Metadata<'_> {
            &TEST_META_1
        }
    }

    struct TestCallsite2;
    static TEST_CALLSITE_2: TestCallsite2 = TestCallsite2;
    static TEST_META_2: Metadata<'static> = metadata! {
        name: "field_test2",
        target: module_path!(),
        level: Level::INFO,
        fields: &[NamedField::new("foo"), NamedField::new("bar"), NamedField::new("baz")],
        callsite: callsite::Identifier(&TEST_CALLSITE_2),
        kind: Kind::SPAN,
    };

    impl crate::callsite::Callsite for TestCallsite2 {
        fn set_interest(&self, _: crate::collect::Interest) {
            unimplemented!()
        }

        fn metadata(&self) -> &Metadata<'_> {
            &TEST_META_2
        }
    }

    struct WriteToStringVisitor {
        result: String,
    }

    impl Visit for WriteToStringVisitor {
        fn visit_named_fields(&mut self, named_values: &NamedValues<'_>) {
            for (_field, value) in named_values.iter() {
                use core::fmt::Write;
                write!(&mut self.result, "{:?}", value).unwrap();
            }
        }

        fn visit_value(&mut self, value: Value<'_>) {
            match value {
                Value::Structable(v) => v.visit(self),
                _ => {} // do nothing for other types
            }
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn value_set_contains_true() {
        #[derive(Valuable)]
        struct MyStruct {
            foo: u32,
            bar: u32,
            baz: u32,
        }

        let my_struct = MyStruct {
            foo: 1,
            bar: 2,
            baz: 3,
        };

        let valueset = ValueSet {
            values: &my_struct,
            callsite: crate::identify_callsite!(&TEST_CALLSITE_1),
        };
        let search_field = NamedField::new("foo");
        assert!(valueset.contains(&search_field));
    }

    #[test]
    #[cfg(feature = "std")]
    fn value_set_contains_false() {
        #[derive(Valuable)]
        struct MyStruct {
            foo: u32,
            bar: u32,
            baz: u32,
        }

        let my_struct = MyStruct {
            foo: 1,
            bar: 2,
            baz: 3,
        };

        let valueset = ValueSet {
            values: &my_struct,
            callsite: crate::identify_callsite!(&TEST_CALLSITE_1),
        };
        let search_field = NamedField::new("quux");
        assert!(!valueset.contains(&search_field));
    }

    #[test]
    #[cfg(feature = "std")]
    fn value_set_is_empty() {
        #[derive(Valuable)]
        struct MyStruct;

        let my_struct = MyStruct;

        let valueset = ValueSet {
            values: &my_struct,
            callsite: crate::identify_callsite!(&TEST_CALLSITE_1),
        };
        assert!(valueset.is_empty());
    }

    #[test]
    #[cfg(feature = "std")]
    fn value_set_is_not_empty() {
        #[derive(Valuable)]
        struct MyStruct {
            foo: u32,
            bar: u32,
            baz: u32,
        }

        let my_struct = MyStruct {
            foo: 1,
            bar: 2,
            baz: 3,
        };

        let valueset = ValueSet {
            values: &my_struct,
            callsite: crate::identify_callsite!(&TEST_CALLSITE_1),
        };
        assert!(!valueset.is_empty());
    }

    #[test]
    #[cfg(feature = "std")]
    fn record_debug_fn() {
        #[derive(Valuable)]
        struct MyStruct {
            foo: u32,
            bar: u32,
            baz: u32,
        }

        let my_struct = MyStruct {
            foo: 1,
            bar: 2,
            baz: 3,
        };

        let valueset = ValueSet {
            values: &my_struct,
            callsite: crate::identify_callsite!(&TEST_CALLSITE_1),
        };
        let mut visitor = WriteToStringVisitor {
            result: String::new(),
        };
        valueset.visit(&mut visitor);
        assert_eq!(visitor.result, String::from("123"));
    }

    struct WriteErrorToStringVisitor {
        result: String,
    }

    impl Visit for WriteErrorToStringVisitor {
        fn visit_named_fields(&mut self, named_values: &NamedValues<'_>) {
            for (_field, value) in named_values.iter() {
                if let Value::Error(e) = value {
                    use core::fmt::Write;
                    write!(&mut self.result, "{}", e).unwrap();
                }
            }
        }

        fn visit_value(&mut self, value: Value<'_>) {
            match value {
                Value::Structable(v) => v.visit(self),
                _ => {} // do nothing for other types
            }
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn record_error() {
        #[derive(Valuable)]
        struct ErrStruct<'a> {
            err: &'a (dyn std::error::Error + 'static),
        }
        let err_struct = ErrStruct {
            err: &std::io::Error::new(std::io::ErrorKind::Other, "lol"),
        };

        let valueset = ValueSet {
            values: &err_struct,
            callsite: crate::identify_callsite!(&TEST_CALLSITE_1),
        };
        let mut visitor = WriteErrorToStringVisitor {
            result: String::new(),
        };
        valueset.visit(&mut visitor);
        assert_eq!(visitor.result, format!("{}", err_struct.err));
    }
}
