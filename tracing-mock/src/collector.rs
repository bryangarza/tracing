#![allow(missing_docs)]
use crate::valuable::NamedValues_;

use super::{
    event::MockEvent,
    span::{MockSpan, NewSpan},
};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
};
use tracing::{
    collect::Interest,
    level_filters::LevelFilter,
    span::{Attributes, Id},
    Collect, Event, Metadata,
};
use valuable::{NamedValues, Value};

#[derive(Debug, Eq, PartialEq)]
pub enum Expect<'a> {
    Event(MockEvent<'a>),
    FollowsFrom {
        consequence: MockSpan,
        cause: MockSpan,
    },
    Enter(MockSpan),
    Exit(MockSpan),
    CloneSpan(MockSpan),
    DropSpan(MockSpan),
    Visit(MockSpan, NamedValues_<'a>),
    NewSpan(NewSpan<'a>),
    Nothing,
}

struct SpanState {
    name: &'static str,
    refs: usize,
    meta: &'static Metadata<'static>,
}

struct Running<'a, F: Fn(&Metadata<'_>) -> bool> {
    spans: Mutex<HashMap<Id, SpanState>>,
    expected: Arc<Mutex<VecDeque<Expect<'a>>>>,
    current: Mutex<Vec<Id>>,
    ids: AtomicUsize,
    max_level: Option<LevelFilter>,
    filter: F,
    name: String,
}

pub struct MockCollector<'a, F: Fn(&Metadata<'_>) -> bool> {
    expected: VecDeque<Expect<'a>>,
    max_level: Option<LevelFilter>,
    filter: F,
    name: String,
}

pub struct MockHandle<'a>(Arc<Mutex<VecDeque<Expect<'a>>>>, String);

pub fn mock() -> MockCollector<'static, fn(&Metadata<'_>) -> bool> {
    MockCollector {
        expected: VecDeque::new(),
        filter: (|_: &Metadata<'_>| true) as for<'r, 's> fn(&'r Metadata<'s>) -> _,
        max_level: None,
        name: thread::current()
            .name()
            .unwrap_or("mock_subscriber")
            .to_string(),
    }
}

impl<'a, F> MockCollector<'a, F>
where
    F: Fn(&Metadata<'_>) -> bool + 'static,
{
    /// Overrides the name printed by the mock subscriber's debugging output.
    ///
    /// The debugging output is displayed if the test panics, or if the test is
    /// run with `--nocapture`.
    ///
    /// By default, the mock subscriber's name is the  name of the test
    /// (*technically*, the name of the thread where it was created, which is
    /// the name of the test unless tests are run with `--test-threads=1`).
    /// When a test has only one mock subscriber, this is sufficient. However,
    /// some tests may include multiple subscribers, in order to test
    /// interactions between multiple subscribers. In that case, it can be
    /// helpful to give each subscriber a separate name to distinguish where the
    /// debugging output comes from.
    pub fn named(self, name: impl ToString) -> Self {
        Self {
            name: name.to_string(),
            ..self
        }
    }

    pub fn enter(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::Enter(span));
        self
    }

    pub fn follows_from(mut self, consequence: MockSpan, cause: MockSpan) -> Self {
        self.expected
            .push_back(Expect::FollowsFrom { consequence, cause });
        self
    }

    pub fn event(mut self, event: MockEvent<'a>) -> Self {
        self.expected.push_back(Expect::Event(event));
        self
    }

    pub fn exit(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::Exit(span));
        self
    }

    pub fn clone_span(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::CloneSpan(span));
        self
    }

    #[allow(deprecated)]
    pub fn drop_span(mut self, span: MockSpan) -> Self {
        self.expected.push_back(Expect::DropSpan(span));
        self
    }

    pub fn done(mut self) -> Self {
        self.expected.push_back(Expect::Nothing);
        self
    }

    pub fn record(mut self, span: MockSpan, fields: NamedValues<'a>) -> Self
    {
        self.expected.push_back(Expect::Visit(span, NamedValues_::new(fields)));
        self
    }

    pub fn new_span<I>(mut self, new_span: I) -> Self
    where
        I: Into<NewSpan<'a>>,
    {
        self.expected.push_back(Expect::NewSpan(new_span.into()));
        self
    }

    pub fn with_filter<G>(self, filter: G) -> MockCollector<'a, G>
    where
        G: Fn(&Metadata<'_>) -> bool + 'a,
    {
        MockCollector {
            expected: self.expected,
            filter,
            max_level: self.max_level,
            name: self.name,
        }
    }

    pub fn with_max_level_hint(self, hint: impl Into<LevelFilter>) -> Self {
        Self {
            max_level: Some(hint.into()),
            ..self
        }
    }

    // pub fn run(self) -> impl Collect {
    //     let (collector, _) = self.run_with_handle();
    //     collector
    // }

    pub fn run_with_handle(self) -> (impl Collect, MockHandle<'a>)
    where
        'a: 'static
    {
        let expected = Arc::new(Mutex::new(self.expected));
        let handle = MockHandle(expected.clone(), self.name.clone());
        let collector = Running {
            spans: Mutex::new(HashMap::new()),
            expected,
            current: Mutex::new(Vec::new()),
            ids: AtomicUsize::new(1),
            filter: self.filter,
            max_level: self.max_level,
            name: self.name,
        };
        (collector, handle)
    }
}

impl<F> Collect for Running<'static, F>
where
    F: Fn(&Metadata<'_>) -> bool + 'static,
{
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        println!("[{}] enabled: {:#?}", self.name, meta);
        let enabled = (self.filter)(meta);
        println!("[{}] enabled -> {}", self.name, enabled);
        enabled
    }

    fn register_callsite(&self, meta: &'static Metadata<'static>) -> Interest {
        println!("[{}] register_callsite: {:#?}", self.name, meta);
        if self.enabled(meta) {
            Interest::always()
        } else {
            Interest::never()
        }
    }
    fn max_level_hint(&self) -> Option<LevelFilter> {
        self.max_level
    }

    fn record(&self, id: &Id, values: NamedValues<'_>) {
        let spans = self.spans.lock().unwrap();
        let mut expected = self.expected.lock().unwrap();
        let span = spans
            .get(id)
            .unwrap_or_else(|| panic!("[{}] no span for ID {:?}", self.name, id));
        println!(
            "[{}] record: {}; id={:?}; values={:?};",
            self.name, span.name, id, values
        );
        let was_expected = matches!(expected.front(), Some(Expect::Visit(_, _)));
        if was_expected {
            if let Expect::Visit(expected_span, expected_values) = expected.pop_front().unwrap()
            {
                if let Some(name) = expected_span.name() {
                    assert_eq!(name, span.name);
                }
                let context = format!("span {}: ", span.name);
                for (expected_field, expected_value) in expected_values.0.lock().unwrap().iter() {
                    let value = values.get_by_name(expected_field.name()).unwrap();
                    match (value, expected_value) {
                        (Value::Bool(v), Value::Bool(e)) => assert_eq!(v, e),
                        (Value::Char(v), Value::Char(e)) => assert_eq!(v, e),
                        (Value::F32(v), Value::F32(e)) => assert_eq!(v, e),
                        (Value::F64(v), Value::F64(e)) => assert_eq!(v, e),
                        (Value::I8(v), Value::I8(e)) => assert_eq!(v, e),
                        (Value::I16(v), Value::I16(e)) => assert_eq!(v, e),
                        (Value::I32(v), Value::I32(e)) => assert_eq!(v, e),
                        (Value::I64(v), Value::I64(e)) => assert_eq!(v, e),
                        (Value::I128(v), Value::I128(e)) => assert_eq!(v, e),
                        (Value::Isize(v), Value::Isize(e)) => assert_eq!(v, e),
                        (Value::String(v), Value::String(e)) => assert_eq!(v, e),
                        (Value::U8(v), Value::U8(e)) => assert_eq!(v, e),
                        (Value::U16(v), Value::U16(e)) => assert_eq!(v, e),
                        (Value::U32(v), Value::U32(e)) => assert_eq!(v, e),
                        (Value::U64(v), Value::U64(e)) => assert_eq!(v, e),
                        (Value::U128(v), Value::U128(e)) => assert_eq!(v, e),
                        (Value::Usize(v), Value::Usize(e)) => assert_eq!(v, e),
                        (Value::Error(_), Value::Error(_)) => unimplemented!(),
                        (Value::Listable(_), Value::Listable(_)) => unimplemented!(),
                        (Value::Mappable(_), Value::Mappable(_)) => unimplemented!(),
                        (Value::Structable(_), Value::Structable(_)) => unimplemented!(),
                        (Value::Enumerable(_), Value::Enumerable(_)) => unimplemented!(),
                        (Value::Tuplable(_), Value::Tuplable(_)) => unimplemented!(),
                        (Value::Unit, Value::Unit) => (),
                        _ => unimplemented!(),
                    }
                }

            }
        }
    }

    fn event(&self, event: &Event<'_>) {
        let name = event.metadata().name();
        println!("[{}] event: {};", self.name, name);
        match self.expected.lock().unwrap().pop_front() {
            None => {}
            Some(Expect::Event(mut expected)) => {
                let get_parent_name = || {
                    let stack = self.current.lock().unwrap();
                    let spans = self.spans.lock().unwrap();
                    event
                        .parent()
                        .and_then(|id| spans.get(id))
                        .or_else(|| stack.last().and_then(|id| spans.get(id)))
                        .map(|s| s.name.to_string())
                };
                expected.check(event, get_parent_name, &self.name);
            }
            Some(ex) => ex.bad(&self.name, format_args!("observed event {:#?}", event)),
        }
    }

    fn record_follows_from(&self, consequence_id: &Id, cause_id: &Id) {
        let spans = self.spans.lock().unwrap();
        if let Some(consequence_span) = spans.get(consequence_id) {
            if let Some(cause_span) = spans.get(cause_id) {
                println!(
                    "[{}] record_follows_from: {} (id={:?}) follows {} (id={:?})",
                    self.name, consequence_span.name, consequence_id, cause_span.name, cause_id,
                );
                match self.expected.lock().unwrap().pop_front() {
                    None => {}
                    Some(Expect::FollowsFrom {
                        consequence: ref expected_consequence,
                        cause: ref expected_cause,
                    }) => {
                        if let Some(name) = expected_consequence.name() {
                            assert_eq!(name, consequence_span.name);
                        }
                        if let Some(name) = expected_cause.name() {
                            assert_eq!(name, cause_span.name);
                        }
                    }
                    Some(ex) => ex.bad(
                        &self.name,
                        format_args!(
                            "consequence {:?} followed cause {:?}",
                            consequence_span.name, cause_span.name
                        ),
                    ),
                }
            }
        };
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        let meta = span.metadata();
        let id = self.ids.fetch_add(1, Ordering::SeqCst);
        let id = Id::from_u64(id as u64);
        println!(
            "[{}] new_span: name={:?}; target={:?}; id={:?};",
            self.name,
            meta.name(),
            meta.target(),
            id
        );
        let mut expected = self.expected.lock().unwrap();
        let was_expected = matches!(expected.front(), Some(Expect::NewSpan(_)));
        let mut spans = self.spans.lock().unwrap();
        if was_expected {
            if let Expect::NewSpan(mut expected) = expected.pop_front().unwrap() {
                let get_parent_name = || {
                    let stack = self.current.lock().unwrap();
                    span.parent()
                        .and_then(|id| spans.get(id))
                        .or_else(|| stack.last().and_then(|id| spans.get(id)))
                        .map(|s| s.name.to_string())
                };
                expected.check(span, get_parent_name, &self.name);
            }
        }
        spans.insert(
            id.clone(),
            SpanState {
                name: meta.name(),
                refs: 1,
                meta,
            },
        );
        id
    }

    fn enter(&self, id: &Id) {
        let spans = self.spans.lock().unwrap();
        if let Some(span) = spans.get(id) {
            println!("[{}] enter: {}; id={:?};", self.name, span.name, id);
            match self.expected.lock().unwrap().pop_front() {
                None => {}
                Some(Expect::Enter(ref expected_span)) => {
                    if let Some(name) = expected_span.name() {
                        assert_eq!(name, span.name);
                    }
                }
                Some(ex) => ex.bad(&self.name, format_args!("entered span {:?}", span.name)),
            }
        };
        self.current.lock().unwrap().push(id.clone());
    }

    fn exit(&self, id: &Id) {
        if std::thread::panicking() {
            // `exit()` can be called in `drop` impls, so we must guard against
            // double panics.
            println!("[{}] exit {:?} while panicking", self.name, id);
            return;
        }
        let spans = self.spans.lock().unwrap();
        let span = spans
            .get(id)
            .unwrap_or_else(|| panic!("[{}] no span for ID {:?}", self.name, id));
        println!("[{}] exit: {}; id={:?};", self.name, span.name, id);
        match self.expected.lock().unwrap().pop_front() {
            None => {}
            Some(Expect::Exit(ref expected_span)) => {
                if let Some(name) = expected_span.name() {
                    assert_eq!(name, span.name);
                }
                let curr = self.current.lock().unwrap().pop();
                assert_eq!(
                    Some(id),
                    curr.as_ref(),
                    "[{}] exited span {:?}, but the current span was {:?}",
                    self.name,
                    span.name,
                    curr.as_ref().and_then(|id| spans.get(id)).map(|s| s.name)
                );
            }
            Some(ex) => ex.bad(&self.name, format_args!("exited span {:?}", span.name)),
        };
    }

    fn clone_span(&self, id: &Id) -> Id {
        let name = self.spans.lock().unwrap().get_mut(id).map(|span| {
            let name = span.name;
            println!(
                "[{}] clone_span: {}; id={:?}; refs={:?};",
                self.name, name, id, span.refs
            );
            span.refs += 1;
            name
        });
        if name.is_none() {
            println!("[{}] clone_span: id={:?};", self.name, id);
        }
        let mut expected = self.expected.lock().unwrap();
        let was_expected = if let Some(Expect::CloneSpan(ref span)) = expected.front() {
            assert_eq!(
                name,
                span.name(),
                "[{}] expected to clone a span named {:?}",
                self.name,
                span.name()
            );
            true
        } else {
            false
        };
        if was_expected {
            expected.pop_front();
        }
        id.clone()
    }

    fn drop_span(&self, id: Id) {
        let mut is_event = false;
        let name = if let Ok(mut spans) = self.spans.try_lock() {
            spans.get_mut(&id).map(|span| {
                let name = span.name;
                if name.contains("event") {
                    is_event = true;
                }
                println!(
                    "[{}] drop_span: {}; id={:?}; refs={:?};",
                    self.name, name, id, span.refs
                );
                span.refs -= 1;
                name
            })
        } else {
            None
        };
        if name.is_none() {
            println!("[{}] drop_span: id={:?}", self.name, id);
        }
        if let Ok(mut expected) = self.expected.try_lock() {
            let was_expected = match expected.front() {
                Some(Expect::DropSpan(ref span)) => {
                    // Don't assert if this function was called while panicking,
                    // as failing the assertion can cause a double panic.
                    if !::std::thread::panicking() {
                        assert_eq!(name, span.name());
                    }
                    true
                }
                Some(Expect::Event(_)) => {
                    if !::std::thread::panicking() {
                        assert!(is_event, "[{}] expected an event", self.name);
                    }
                    true
                }
                _ => false,
            };
            if was_expected {
                expected.pop_front();
            }
        }
    }

    fn current_span(&self) -> tracing_core::span::Current {
        let stack = self.current.lock().unwrap();
        match stack.last() {
            Some(id) => {
                let spans = self.spans.lock().unwrap();
                let state = spans.get(id).expect("state for current span");
                tracing_core::span::Current::new(id.clone(), state.meta)
            }
            None => tracing_core::span::Current::none(),
        }
    }
}

impl<'a> MockHandle<'a> {
    pub fn new(expected: Arc<Mutex<VecDeque<Expect<'a>>>>, name: String) -> Self {
        Self(expected, name)
    }

    pub fn assert_finished(&self) {
        if let Ok(ref expected) = self.0.lock() {
            assert!(
                !expected.iter().any(|thing| thing != &Expect::Nothing),
                "\n[{}] more notifications expected: {:#?}",
                self.1,
                **expected
            );
        }
    }
}

impl<'a> Expect<'a> {
    pub fn bad(&self, name: impl AsRef<str>, what: fmt::Arguments<'_>) {
        let name = name.as_ref();
        match self {
            Expect::Event(e) => panic!(
                "\n[{}] expected event {}\n[{}] but instead {}",
                name, e, name, what,
            ),
            Expect::FollowsFrom { consequence, cause } => panic!(
                "\n[{}] expected consequence {} to follow cause {} but instead {}",
                name, consequence, cause, what,
            ),
            Expect::Enter(e) => panic!(
                "\n[{}] expected to enter {}\n[{}] but instead {}",
                name, e, name, what,
            ),
            Expect::Exit(e) => panic!(
                "\n[{}] expected to exit {}\n[{}] but instead {}",
                name, e, name, what,
            ),
            Expect::CloneSpan(e) => {
                panic!(
                    "\n[{}] expected to clone {}\n[{}] but instead {}",
                    name, e, name, what,
                )
            }
            Expect::DropSpan(e) => {
                panic!(
                    "\n[{}] expected to drop {}\n[{}] but instead {}",
                    name, e, name, what,
                )
            }
            Expect::Visit(e, fields) => panic!(
                "\n[{}] expected {} to record {:?}\n[{}] but instead {}",
                name, e, fields, name, what,
            ),
            Expect::NewSpan(e) => panic!(
                "\n[{}] expected {}\n[{}] but instead {}",
                name, e, name, what
            ),
            Expect::Nothing => panic!(
                "\n[{}] expected nothing else to happen\n[{}] but {} instead",
                name, name, what,
            ),
        }
    }
}
