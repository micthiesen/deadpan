use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Event {
    operation: &'static str,
    units: usize,
    accepted: bool,
}
#[derive(Clone)]
struct Caller {
    state: Rc<RefCell<PolicyState>>,
    before: Option<[usize; 6]>,
}
impl Caller {
    fn check(&self) {
        assert_eq!(caller_snapshot(Some(&self.state)), self.before);
    }
}
struct Recording {
    caller: Option<Caller>,
    policy: Policy<true>,
    events: Vec<Event>,
    states: Vec<Rc<RefCell<PolicyState>>>,
    spend_calls: usize,
    cancel_work_call: Option<usize>,
    merges: usize,
    rotate_merge: Option<usize>,
}
impl Recording {
    fn new(caller: Option<&Rc<RefCell<PolicyState>>>) -> Self {
        let policy = Policy::new(usize::MAX);
        Self {
            caller: caller.map(|state| Caller {
                state: Rc::clone(state),
                before: caller_snapshot(Some(state)),
            }),
            states: vec![Rc::clone(&policy.0)],
            policy,
            events: Vec::new(),
            spend_calls: 0,
            cancel_work_call: None,
            merges: 0,
            rotate_merge: None,
        }
    }
    fn check_caller(&self) {
        if let Some(caller) = &self.caller {
            caller.check();
        }
    }
    fn remember(&mut self) {
        self.check_caller();
        if !Rc::ptr_eq(self.states.last().unwrap(), &self.policy.0) {
            self.states.push(Rc::clone(&self.policy.0));
        }
    }
    fn assert_released(&self) {
        self.check_caller();
        for state in &self.states {
            assert_eq!(state.borrow().live, 0);
        }
        assert_eq!(
            self.states
                .iter()
                .map(|state| state.borrow().forwarded)
                .sum::<usize>(),
            self.events
                .iter()
                .filter(|event| event.accepted && event.operation == "spend")
                .map(|event| event.units)
                .sum::<usize>(),
            "accepted failed-prefix work is retained on its actual issuer"
        );
    }
}
impl AudioSoundContextAdmission for Recording {
    const METERED: bool = true;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.policy.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.check_caller();
        self.spend_calls += 1;
        let result = if self.cancel_work_call == Some(self.spend_calls) {
            Err(PlanError::AudioQueryLimit("test wide cancellation"))
        } else {
            self.policy.spend(units)
        };
        self.events.push(Event {
            operation: "spend",
            units,
            accepted: result.is_ok(),
        });
        self.remember();
        result
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        self.check_caller();
        let result = self.policy.reserve(layout);
        self.events.push(Event {
            operation: "reserve",
            units: layout.size(),
            accepted: result.is_ok(),
        });
        self.remember();
        result
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.check_caller();
        self.policy.merge(target, source);
        self.merges += 1;
        self.events.push(Event {
            operation: "merge",
            units: 0,
            accepted: true,
        });
        if self.rotate_merge == Some(self.merges) {
            self.policy.rotate(usize::MAX);
        }
        self.remember();
    }
}
pub(super) fn check(
    retained: &AudioOwnerRetainedInput<'_>,
    row: &Value,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> usize {
    let requested = range(row);
    let mut baseline = Recording::new(caller);
    let query = retained
        .fades_admitted(requested.clone(), Default::default(), &mut baseline)
        .unwrap();
    assert_true(&query, &row["output"]["query"], &baseline.policy);
    drop(query);
    baseline.assert_released();
    let expected = baseline.events;
    let mut attempts = 0;
    for operation in ["spend", "reserve"] {
        let mut ordinal = 0;
        for (index, event) in expected
            .iter()
            .enumerate()
            .filter(|(_, event)| event.operation == operation)
        {
            ordinal += 1;
            let mut recorder = Recording::new(caller);
            if operation == "spend" {
                recorder.policy.0.borrow_mut().refuse_work_call = Some(ordinal);
            } else {
                recorder.policy.0.borrow_mut().refuse_reservation = Some(ordinal);
            }
            let result =
                retained.fades_admitted(requested.clone(), Default::default(), &mut recorder);
            let expected_error = if operation == "spend" {
                "test wide work refusal"
            } else {
                "test wide reservation refusal"
            };
            assert!(
                matches!(&result, Err(PlanError::AudioQueryLimit(label)) if *label == expected_error)
            );
            drop(result);
            assert_eq!(&recorder.events[..index], &expected[..index]);
            assert_eq!(
                recorder.events.len(),
                index + 1,
                "no later callback after refusal"
            );
            assert_eq!(
                recorder.events[index],
                Event {
                    accepted: false,
                    ..*event
                }
            );
            recorder.assert_released();
            attempts += 1;
        }
    }
    for operation in ["spend", "reserve", "merge"] {
        let mut ordinal = 0;
        for (index, _) in expected
            .iter()
            .enumerate()
            .filter(|(_, event)| event.operation == operation)
        {
            ordinal += 1;
            let mut recorder = Recording::new(caller);
            match operation {
                "spend" => recorder.policy.0.borrow_mut().rotate_work_call = Some(ordinal),
                "reserve" => recorder.policy.0.borrow_mut().rotate_reservation = Some(ordinal),
                "merge" => recorder.rotate_merge = Some(ordinal),
                _ => unreachable!(),
            }
            let result =
                retained.fades_admitted(requested.clone(), Default::default(), &mut recorder);
            assert!(
                matches!(&result, Err(PlanError::AudioMetadataIssuerMismatch)),
                "{operation} {ordinal}"
            );
            drop(result);
            assert_eq!(
                recorder.events,
                expected[..=index],
                "accepted rotation callback is the last"
            );
            assert_eq!(recorder.states.len(), 2);
            recorder.assert_released();
            attempts += 1;
        }
    }
    let mut spend_ordinal = 0;
    for (index, event) in expected
        .iter()
        .enumerate()
        .filter(|(_, event)| event.operation == "spend")
    {
        let mut recorder = Recording::new(caller);
        spend_ordinal += 1;
        recorder.cancel_work_call = Some(spend_ordinal);
        let result = retained.fades_admitted(requested.clone(), Default::default(), &mut recorder);
        assert!(matches!(
            &result,
            Err(PlanError::AudioQueryLimit("test wide cancellation"))
        ));
        drop(result);
        assert_eq!(recorder.events.len(), index + 1);
        assert_eq!(
            recorder.events[index],
            Event {
                accepted: false,
                ..*event
            }
        );
        assert_eq!(&recorder.events[..index], &expected[..index]);
        recorder.assert_released();
        attempts += 1;
    }
    attempts
}
