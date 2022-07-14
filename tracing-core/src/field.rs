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
use crate::callsite;
use core::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    num,
    ops::Range,
};

use self::private::ValidLen;
use valuable::{Valuable, Value, Visit};

/// An opaque key allowing _O_(1) access to a field in a `Span`'s key-value
/// data.
///
/// As keys are defined by the _metadata_ of a span, rather than by an
/// individual instance of a span, a key may be used to access the same field
/// across all instances of a given span with the same metadata. Thus, when a
/// collector observes a new span, it need only access a field by name _once_,
/// and use the key for that name for all other accesses.
#[derive(Debug)]
pub struct Field {
    i: usize,
    fields: FieldSet,
}

/// An empty field.
///
/// This can be used to indicate that the value of a field is not currently
/// present but will be recorded later.
///
/// When a field's value is `Empty`. it will not be recorded.
#[derive(Debug, Eq, PartialEq)]
pub struct Empty;

/// Describes the fields present on a span.
pub struct FieldSet {
    /// The names of each field on the described span.
    names: &'static [&'static str],
    /// The callsite where the described span originates.
    callsite: callsite::Identifier,
}

/// A set of fields and values for a span.
pub struct ValueSet<'a> {
    values: &'a [(&'a Field, Option<&'a (dyn Valuable + 'a)>)],
    fields: &'a FieldSet,
}

/// An iterator over a set of fields.
#[derive(Debug)]
pub struct Iter {
    idxs: Range<usize>,
    fields: FieldSet,
}

// ===== impl Field =====

impl Field {
    /// Returns an [`Identifier`] that uniquely identifies the [`Callsite`]
    /// which defines this field.
    ///
    /// [`Identifier`]: super::callsite::Identifier
    /// [`Callsite`]: super::callsite::Callsite
    #[inline]
    pub fn callsite(&self) -> callsite::Identifier {
        self.fields.callsite()
    }

    /// Returns a string representing the name of the field.
    pub fn name(&self) -> &'static str {
        self.fields.names[self.i]
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.name())
    }
}

impl AsRef<str> for Field {
    fn as_ref(&self) -> &str {
        self.name()
    }
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        self.callsite() == other.callsite() && self.i == other.i
    }
}

impl Eq for Field {}

impl Hash for Field {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.callsite().hash(state);
        self.i.hash(state);
    }
}

impl Clone for Field {
    fn clone(&self) -> Self {
        Field {
            i: self.i,
            fields: FieldSet {
                names: self.fields.names,
                callsite: self.fields.callsite(),
            },
        }
    }
}

// ===== impl FieldSet =====

impl FieldSet {
    /// Constructs a new `FieldSet` with the given array of field names and callsite.
    pub const fn new(names: &'static [&'static str], callsite: callsite::Identifier) -> Self {
        Self { names, callsite }
    }

    /// Returns an [`Identifier`] that uniquely identifies the [`Callsite`]
    /// which defines this set of fields..
    ///
    /// [`Identifier`]: super::callsite::Identifier
    /// [`Callsite`]: super::callsite::Callsite
    pub(crate) fn callsite(&self) -> callsite::Identifier {
        callsite::Identifier(self.callsite.0)
    }

    /// Returns the [`Field`] named `name`, or `None` if no such field exists.
    ///
    /// [`Field`]: super::Field
    pub fn field<Q: ?Sized>(&self, name: &Q) -> Option<Field>
    where
        Q: Borrow<str>,
    {
        let name = &name.borrow();
        self.names.iter().position(|f| f == name).map(|i| Field {
            i,
            fields: FieldSet {
                names: self.names,
                callsite: self.callsite(),
            },
        })
    }

    /// Returns `true` if `self` contains the given `field`.
    ///
    /// <div class="example-wrap" style="display:inline-block">
    /// <pre class="ignore" style="white-space:normal;font:inherit;">
    /// <strong>Note</strong>: If <code>field</code> shares a name with a field
    /// in this <code>FieldSet</code>, but was created by a <code>FieldSet</code>
    /// with a different callsite, this <code>FieldSet</code> does <em>not</em>
    /// contain it. This is so that if two separate span callsites define a field
    /// named "foo", the <code>Field</code> corresponding to "foo" for each
    /// of those callsites are not equivalent.
    /// </pre></div>
    pub fn contains(&self, field: &Field) -> bool {
        field.callsite() == self.callsite() && field.i <= self.len()
    }

    /// Returns an iterator over the `Field`s in this `FieldSet`.
    pub fn iter(&self) -> Iter {
        let idxs = 0..self.len();
        Iter {
            idxs,
            fields: FieldSet {
                names: self.names,
                callsite: self.callsite(),
            },
        }
    }

    /// Returns a new `ValueSet` with entries for this `FieldSet`'s values.
    ///
    /// Note that a `ValueSet` may not be constructed with arrays of over 32
    /// elements.
    #[doc(hidden)]
    pub fn value_set<'v, V>(&'v self, values: &'v V) -> ValueSet<'v>
    where
        V: ValidLen<'v>,
    {
        ValueSet {
            fields: self,
            values: values.borrow(),
        }
    }

    /// Returns the number of fields in this `FieldSet`.
    #[inline]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns whether or not this `FieldSet` has fields.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl<'a> IntoIterator for &'a FieldSet {
    type IntoIter = Iter;
    type Item = Field;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

// TODO: Get this working with Valuable
impl fmt::Debug for FieldSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FieldSet")
            // .field("names", &self.names)
            // .field("callsite", &self.callsite)
            .finish()
    }
}

// TODO: Get this working with Valuable
impl fmt::Display for FieldSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set()
            // .entries(self.names.iter().map(display))
            .finish()
    }
}

// ===== impl Iter =====

impl Iterator for Iter {
    type Item = Field;
    fn next(&mut self) -> Option<Field> {
        let i = self.idxs.next()?;
        Some(Field {
            i,
            fields: FieldSet {
                names: self.fields.names,
                callsite: self.fields.callsite(),
            },
        })
    }
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
        self.fields.callsite()
    }

    /// Visits all the fields in this `ValueSet` with the provided [visitor].
    ///
    /// [visitor]: Visit
    pub fn record(&self, visitor: &mut dyn Visit) {
        let my_callsite = self.callsite();
        for (field, value) in self.values {
            if field.callsite() != my_callsite {
                continue;
            }
            if let Some(value) = value {
                value.visit(visitor);
            }
        }
    }

    /// Returns `true` if this `ValueSet` contains a value for the given `Field`.
    pub(crate) fn contains(&self, field: &Field) -> bool {
        field.callsite() == self.callsite()
            && self
                .values
                .iter()
                .any(|(key, val)| *key == field && val.is_some())
    }

    /// Returns true if this `ValueSet` contains _no_ values.
    pub(crate) fn is_empty(&self) -> bool {
        let my_callsite = self.callsite();
        self.values
            .iter()
            .all(|(key, val)| val.is_none() || key.callsite() != my_callsite)
    }

    pub(crate) fn field_set(&self) -> &FieldSet {
        self.fields
    }
}

// TODO: Get this working with Valuable
impl<'a> fmt::Debug for ValueSet<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // self.values
        //     .iter()
        //     .fold(&mut f.debug_struct("ValueSet"), |dbg, (key, v)| {
        //         if let Some(val) = v {
        //             val.visit(dbg);
        //         }
        //         dbg
        //     })
        //     .field("callsite", &self.callsite())
        //     .finish()
        fmt::Result::Ok(())
    }
}

// impl<'a> fmt::Display for ValueSet<'a> {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         self.values
//             .iter()
//             .fold(&mut f.debug_map(), |dbg, (key, v)| {
//                 if let Some(val) = v {
//                     val.visit(dbg);
//                 }
//                 dbg
//             })
//             .finish()
//     }
// }

// ===== impl ValidLen =====

mod private {
    use super::*;

    /// Marker trait implemented by arrays which are of valid length to
    /// construct a `ValueSet`.
    ///
    /// `ValueSet`s may only be constructed from arrays containing 32 or fewer
    /// elements, to ensure the array is small enough to always be allocated on the
    /// stack. This trait is only implemented by arrays of an appropriate length,
    /// ensuring that the correct size arrays are used at compile-time.
    pub trait ValidLen<'a>: Borrow<[(&'a Field, Option<&'a (dyn Valuable + 'a)>)]> {}
}

macro_rules! impl_valid_len {
    ( $( $len:tt ),+ ) => {
        $(
            impl<'a> private::ValidLen<'a> for
                [(&'a Field, Option<&'a (dyn Valuable + 'a)>); $len] {}
        )+
    }
}

impl_valid_len! {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::metadata::{Kind, Level, Metadata};

    struct TestCallsite1;
    static TEST_CALLSITE_1: TestCallsite1 = TestCallsite1;
    static TEST_META_1: Metadata<'static> = metadata! {
        name: "field_test1",
        target: module_path!(),
        level: Level::INFO,
        fields: &["foo", "bar", "baz"],
        callsite: &TEST_CALLSITE_1,
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
        fields: &["foo", "bar", "baz"],
        callsite: &TEST_CALLSITE_2,
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

    #[test]
    fn value_set_with_no_values_is_empty() {
        let fields = TEST_META_1.fields();
        let values = &[
            (&fields.field("foo").unwrap(), None),
            (&fields.field("bar").unwrap(), None),
            (&fields.field("baz").unwrap(), None),
        ];
        let valueset = fields.value_set(values);
        assert!(valueset.is_empty());
    }

    #[test]
    fn empty_value_set_is_empty() {
        let fields = TEST_META_1.fields();
        let valueset = fields.value_set(&[]);
        assert!(valueset.is_empty());
    }

    // #[test]
    // fn value_sets_with_fields_from_other_callsites_are_empty() {
    //     let fields = TEST_META_1.fields();
    //     let values = &[
    //         (&fields.field("foo").unwrap(), Some(&1)),
    //         (&fields.field("bar").unwrap(), Some(&2)),
    //         (&fields.field("baz").unwrap(), Some(&3)),
    //     ];
    //     let valueset = TEST_META_2.fields().value_set(values);
    //     assert!(valueset.is_empty())
    // }

    // #[test]
    // fn sparse_value_sets_are_not_empty() {
    //     let fields = TEST_META_1.fields();
    //     let values = &[
    //         (&fields.field("foo").unwrap(), None),
    //         (&fields.field("bar").unwrap(), Some(&57)),
    //         (&fields.field("baz").unwrap(), None),
    //     ];
    //     let valueset = fields.value_set(values);
    //     assert!(!valueset.is_empty());
    // }

    // #[test]
    // fn fields_from_other_callsets_are_skipped() {
    //     let fields = TEST_META_1.fields();
    //     let values = &[
    //         (&fields.field("foo").unwrap(), None),
    //         (
    //             &TEST_META_2.fields().field("bar").unwrap(),
    //             Some(&57),
    //         ),
    //         (&fields.field("baz").unwrap(), None),
    //     ];

    //     struct MyVisitor;
    //     impl Visit for MyVisitor {
    //         fn record_debug(&mut self, field: &Field, _: &dyn (core::fmt::Debug)) {
    //             assert_eq!(field.callsite(), TEST_META_1.callsite())
    //         }
    //     }
    //     let valueset = fields.value_set(values);
    //     valueset.record(&mut MyVisitor);
    // }

    // #[test]
    // fn empty_fields_are_skipped() {
    //     let fields = TEST_META_1.fields();
    //     let values = &[
    //         (&fields.field("foo").unwrap(), Some(&Empty)),
    //         (&fields.field("bar").unwrap(), Some(&57)),
    //         (&fields.field("baz").unwrap(), Some(&Empty)),
    //     ];

    //     struct MyVisitor;
    //     impl Visit for MyVisitor {
    //         fn record_debug(&mut self, field: &Field, _: &dyn (core::fmt::Debug)) {
    //             assert_eq!(field.name(), "bar")
    //         }
    //     }
    //     let valueset = fields.value_set(values);
    //     valueset.record(&mut MyVisitor);
    // }

    // #[test]
    // #[cfg(feature = "std")]
    // fn record_debug_fn() {
    //     let fields = TEST_META_1.fields();
    //     let values = &[
    //         (&fields.field("foo").unwrap(), Some(&1)),
    //         (&fields.field("bar").unwrap(), Some(&2)),
    //         (&fields.field("baz").unwrap(), Some(&3)),
    //     ];
    //     let valueset = fields.value_set(values);
    //     let mut result = String::new();
    //     valueset.record(&mut |_: &Field, value: &dyn fmt::Debug| {
    //         use core::fmt::Write;
    //         write!(&mut result, "{:?}", value).unwrap();
    //     });
    //     assert_eq!(result, String::from("123"));
    // }

    // #[test]
    // #[cfg(feature = "std")]
    // fn record_error() {
    //     let fields = TEST_META_1.fields();
    //     let err: Box<dyn std::error::Error + Send + Sync + 'static> =
    //         std::io::Error::new(std::io::ErrorKind::Other, "lol").into();
    //     let values = &[
    //         (&fields.field("foo").unwrap(), Some(&err)),
    //         (&fields.field("bar").unwrap(), Some(&Empty)),
    //         (&fields.field("baz").unwrap(), Some(&Empty)),
    //     ];
    //     let valueset = fields.value_set(values);
    //     let mut result = String::new();
    //     valueset.record(&mut |_: &Field, value: &dyn fmt::Debug| {
    //         use core::fmt::Write;
    //         write!(&mut result, "{:?}", value).unwrap();
    //     });
    //     assert_eq!(result, format!("{}", err));
    // }
}
