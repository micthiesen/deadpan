//! Observation wrapper over the existing owning Policy and Lease.
use super::*;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub(in crate::plan) enum Fault {
    None,
    Cap(usize),
    RefuseSpend(usize),
    RefuseReserve(usize),
    CancelAfter(usize),
    RotateReserve(usize),
    RotateSpend(usize),
}
pub(in crate::plan) struct Lease(super::Lease);
pub(in crate::plan) struct Backend {
    inner: Policy<true>,
    events: Rc<RefCell<Vec<Value>>>,
}
#[derive(Clone)]
pub(in crate::plan) struct Observer {
    state: Rc<RefCell<PolicyState>>,
    events: Rc<RefCell<Vec<Value>>>,
}
fn counters(state: &PolicyState) -> Value {
    json!({"live":state.live,"peak":state.peak,"cap":state.cap,"reservations":state.reservations,
        "forwarded":state.forwarded,"work_calls":state.work_calls})
}
impl Backend {
    pub(in crate::plan) fn new(fault: Fault) -> Self {
        let inner = Policy::new(match fault {
            Fault::Cap(cap) => cap,
            _ => usize::MAX,
        });
        match fault {
            Fault::RefuseSpend(at) => inner.0.borrow_mut().refuse_work_call = Some(at),
            Fault::RefuseReserve(at) => inner.0.borrow_mut().refuse_reservation = Some(at),
            Fault::CancelAfter(at) => inner.0.borrow_mut().cancel_after = Some(at),
            Fault::RotateReserve(at) => inner.0.borrow_mut().rotate_reservation = Some(at),
            Fault::RotateSpend(at) => inner.0.borrow_mut().rotate_work_call = Some(at),
            _ => {}
        }
        Self {
            inner,
            events: Rc::new(RefCell::new(Vec::new())),
        }
    }
    pub(in crate::plan) fn observer(&self) -> Observer {
        Observer {
            state: Rc::clone(&self.inner.0),
            events: Rc::clone(&self.events),
        }
    }
}
impl Observer {
    pub(in crate::plan) fn snapshot(&self) -> Value {
        counters(&self.state.borrow())
    }
    pub(in crate::plan) fn events(&self) -> Vec<Value> {
        self.events.borrow().clone()
    }
    pub(in crate::plan) fn live(&self) -> usize {
        self.state.borrow().live
    }
    pub(in crate::plan) fn forwarded(&self) -> usize {
        self.state.borrow().forwarded
    }
    pub(in crate::plan) fn spends(&self) -> usize {
        self.state.borrow().work_calls
    }
    pub(in crate::plan) fn reservations(&self) -> usize {
        self.state.borrow().reservations
    }
    pub(in crate::plan) fn assert_released(&self) {
        assert_eq!(self.live(), 0, "actual existing Policy leases remain live");
        let accepted = self
            .events
            .borrow()
            .iter()
            .filter(|event| event["event"] == "spend" && event["accepted"] == true)
            .map(|event| event["units"].as_u64().unwrap() as usize)
            .sum::<usize>();
        assert_eq!(
            accepted,
            self.forwarded(),
            "accepted callback units forwarded once, never refunded"
        );
    }
}
impl AudioSoundContextAdmission for Backend {
    const METERED: bool = true;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.inner.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        let original = Rc::clone(&self.inner.0);
        let before = counters(&original.borrow());
        let result = self.inner.spend(units);
        self.events.borrow_mut().push(
            json!({"event":"spend","units":units,"accepted":result.is_ok(),
            "before":before,"after":counters(&self.inner.0.borrow()),
            "original_issuer_after":counters(&original.borrow()),
            "issuer_changed":!Rc::ptr_eq(&original,&self.inner.0)}),
        );
        result
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let original = Rc::clone(&self.inner.0);
        let before = counters(&original.borrow());
        let result = self.inner.reserve(layout);
        self.events.borrow_mut().push(
            json!({"event":"reserve","size":layout.size(),"align":layout.align(),
            "accepted":result.is_ok(),"before":before,"after":counters(&self.inner.0.borrow()),
            "original_issuer_after":counters(&original.borrow()),
            "issuer_changed":!Rc::ptr_eq(&original,&self.inner.0)}),
        );
        result.map(Lease)
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        let before = counters(&self.inner.0.borrow());
        let target_bytes = target.0.bytes;
        let source_bytes = source.0.bytes;
        self.inner.merge(&mut target.0, &mut source.0);
        self.events.borrow_mut().push(
            json!({"event":"merge","target_bytes":target_bytes,"source_bytes":source_bytes,
            "after_target_bytes":target.0.bytes,"after_source_bytes":source.0.bytes,
            "before":before,"after":counters(&self.inner.0.borrow())}),
        );
    }
}
