//! Single-threaded reactive runtime for RustFlow.
//!
//! The runtime uses `Rc<RefCell<_>>` intentionally: handles are neither
//! `Send` nor `Sync`. Effects run eagerly, track reads dynamically, and are
//! drained by a FIFO scheduler. Use `Runtime::batch` to coalesce writes.
//!
//! Signals created with `Runtime::signal` always notify on writes. Use
//! `Runtime::signal_eq` when `PartialEq` equality suppression is desired.

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fmt,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    rc::{Rc, Weak},
};

type RuntimeRef = Rc<RefCell<RuntimeInner>>;
type SignalId = u64;
type EffectId = u64;
type ScopeId = u64;
type Cleanup = Box<dyn FnOnce()>;

/// Deterministic failures from invalid or disposed runtime handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    RuntimeDisposed,
    InvalidSignal,
    InvalidEffect,
    InvalidScope,
    DisposedScope,
    CycleDetected { limit: usize },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeDisposed => write!(f, "reactive runtime is disposed"),
            Self::InvalidSignal => write!(f, "signal does not belong to a live runtime"),
            Self::InvalidEffect => write!(f, "effect is invalid or disposed"),
            Self::InvalidScope => write!(f, "owner scope is invalid"),
            Self::DisposedScope => write!(f, "owner scope is disposed"),
            Self::CycleDetected { limit } => write!(
                f,
                "reactive cycle detected: scheduler exceeded {limit} effect executions in one flush; pending work was cleared"
            ),
        }
    }
}

impl std::error::Error for RuntimeError {}

struct SignalNode {
    subscribers: BTreeSet<EffectId>,
}

struct EffectNode {
    callback: Rc<RefCell<Box<dyn FnMut()>>>,
    dependencies: BTreeSet<SignalId>,
    owner: ScopeId,
}

struct ScopeNode {
    children: Vec<ScopeId>,
    effects: Vec<EffectId>,
    cleanups: Vec<Cleanup>,
    disposed: bool,
}

struct RuntimeInner {
    signals: HashMap<SignalId, SignalNode>,
    effects: HashMap<EffectId, EffectNode>,
    scopes: HashMap<ScopeId, ScopeNode>,
    next_signal: SignalId,
    next_effect: EffectId,
    next_scope: ScopeId,
    root_scope: ScopeId,
    active_effect: Option<EffectId>,
    active_scope: Option<ScopeId>,
    queue: VecDeque<EffectId>,
    pending: HashSet<EffectId>,
    batch_depth: usize,
    flushing: bool,
    disposed: bool,
    cycle_limit: usize,
}

/// A single-threaded reactive runtime. Clones share the same runtime state.
#[derive(Clone)]
pub struct Runtime {
    inner: RuntimeRef,
    root: Scope,
}

/// A typed, runtime-bound reactive value.
///
/// `get` tracks the current effect, if any. `set` is equality-aware only for
/// signals created with `Runtime::signal_eq`.
pub struct Signal<T: Clone + 'static> {
    id: SignalId,
    runtime: Weak<RefCell<RuntimeInner>>,
    value: Rc<RefCell<T>>,
    equality: Option<Rc<dyn Fn(&T, &T) -> bool>>,
}

impl<T: Clone + 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            runtime: self.runtime.clone(),
            value: self.value.clone(),
            equality: self.equality.clone(),
        }
    }
}

/// A handle for explicitly disposing an effect. Effects are also owned by a scope.
pub struct Effect {
    id: EffectId,
    runtime: Weak<RefCell<RuntimeInner>>,
}

/// An ownership scope for effects, child scopes, and cleanup callbacks.
#[derive(Clone)]
pub struct Scope {
    id: ScopeId,
    runtime: Weak<RefCell<RuntimeInner>>,
}

impl Runtime {
    /// Create an isolated runtime with the default 1,000-execution cycle guard.
    pub fn new() -> Self {
        Self::with_cycle_limit(1_000)
    }

    /// Create a runtime with a custom per-flush effect execution limit.
    pub fn with_cycle_limit(cycle_limit: usize) -> Self {
        let root_scope = 1;
        let mut scopes = HashMap::new();
        scopes.insert(
            root_scope,
            ScopeNode {
                children: Vec::new(),
                effects: Vec::new(),
                cleanups: Vec::new(),
                disposed: false,
            },
        );
        let inner = Rc::new(RefCell::new(RuntimeInner {
            signals: HashMap::new(),
            effects: HashMap::new(),
            scopes,
            next_signal: 1,
            next_effect: 1,
            next_scope: 2,
            root_scope,
            active_effect: None,
            active_scope: None,
            queue: VecDeque::new(),
            pending: HashSet::new(),
            batch_depth: 0,
            flushing: false,
            disposed: false,
            cycle_limit: cycle_limit.max(1),
        }));
        let root = Scope {
            id: root_scope,
            runtime: Rc::downgrade(&inner),
        };
        Self { inner, root }
    }

    /// Return the runtime's root owner scope.
    pub fn root_scope(&self) -> Scope {
        self.root.clone()
    }

    /// Create a child scope under the active owner, or under the root if called externally.
    pub fn scope(&self) -> Result<Scope, RuntimeError> {
        let parent = {
            let inner = self.inner.borrow();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            inner.active_scope.unwrap_or(inner.root_scope)
        };
        create_scope(&self.inner, parent)
    }

    /// Create a signal whose writes always notify subscribers.
    ///
    /// This is the fallback for types without `PartialEq`.
    pub fn signal<T: Clone + 'static>(&self, value: T) -> Result<Signal<T>, RuntimeError> {
        self.create_signal(value, None)
    }

    /// Create a signal that suppresses writes when old and new values compare equal.
    pub fn signal_eq<T: Clone + PartialEq + 'static>(
        &self,
        value: T,
    ) -> Result<Signal<T>, RuntimeError> {
        self.create_signal(value, Some(Rc::new(|old, new| old == new)))
    }

    fn create_signal<T: Clone + 'static>(
        &self,
        value: T,
        equality: Option<Rc<dyn Fn(&T, &T) -> bool>>,
    ) -> Result<Signal<T>, RuntimeError> {
        let id = {
            let mut inner = self.inner.borrow_mut();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            let id = inner.next_signal;
            inner.next_signal += 1;
            inner.signals.insert(id, SignalNode { subscribers: BTreeSet::new() });
            id
        };
        Ok(Signal {
            id,
            runtime: Rc::downgrade(&self.inner),
            value: Rc::new(RefCell::new(value)),
            equality,
        })
    }

    /// Create an eager effect owned by the active scope, or by the root scope.
    pub fn effect(&self, callback: impl FnMut() + 'static) -> Result<Effect, RuntimeError> {
        let owner = {
            let inner = self.inner.borrow();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            inner.active_scope.unwrap_or(inner.root_scope)
        };
        create_effect(&self.inner, owner, Box::new(callback))
    }

    /// Execute a closure as one batch. Nested batches flush only at the outer boundary.
    ///
    /// User panics are propagated after batch bookkeeping is restored. Scheduler cycle
    /// diagnostics are returned as `Err`.
    pub fn batch<R>(&self, callback: impl FnOnce() -> R) -> Result<R, RuntimeError> {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            inner.batch_depth += 1;
        }

        let result = catch_unwind(AssertUnwindSafe(callback));
        let should_flush = {
            let mut inner = self.inner.borrow_mut();
            inner.batch_depth -= 1;
            inner.batch_depth == 0
        };

        let flush_result = if should_flush {
            catch_unwind(AssertUnwindSafe(|| flush_runtime(&self.inner)))
        } else {
            Ok(Ok(()))
        };

        match (result, flush_result) {
            (Err(payload), _) => resume_unwind(payload),
            (Ok(_), Err(payload)) => resume_unwind(payload),
            (Ok(value), Ok(Err(error))) => Err(error),
            (Ok(value), Ok(Ok(()))) => Ok(value),
        }
    }

    /// Drain queued effects until quiescent or the cycle guard trips.
    pub fn flush(&self) -> Result<(), RuntimeError> {
        flush_runtime(&self.inner)
    }

    /// Dispose the root scope and invalidate all handles belonging to this runtime.
    pub fn dispose(&self) -> Result<(), RuntimeError> {
        {
            let inner = self.inner.borrow();
            if inner.disposed {
                return Ok(());
            }
        }
        dispose_scope(&self.inner, self.root.id)?;
        let mut inner = self.inner.borrow_mut();
        inner.disposed = true;
        inner.queue.clear();
        inner.pending.clear();
        Ok(())
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// Read the value and track this signal in the currently executing effect.
    pub fn try_get(&self) -> Result<T, RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        {
            let inner = runtime.borrow();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            if !inner.signals.contains_key(&self.id) {
                return Err(RuntimeError::InvalidSignal);
            }
        }
        let value = self.value.borrow().clone();
        track_read(&runtime, self.id)?;
        Ok(value)
    }

    /// Read the value, panicking with a diagnostic if the runtime is invalid.
    pub fn get(&self) -> T {
        self.try_get().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Write the value. Returns `true` when the value changed and `false` for
    /// equality-suppressed writes. Effects are flushed after the write completes.
    pub fn try_set(&self, value: T) -> Result<bool, RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        {
            let inner = runtime.borrow();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            if !inner.signals.contains_key(&self.id) {
                return Err(RuntimeError::InvalidSignal);
            }
        }

        if let Some(equal) = &self.equality {
            if equal(&*self.value.borrow(), &value) {
                return Ok(false);
            }
        }
        *self.value.borrow_mut() = value;

        {
            let mut inner = runtime.borrow_mut();
            let subscribers = inner
                .signals
                .get(&self.id)
                .ok_or(RuntimeError::InvalidSignal)?
                .subscribers
                .iter()
                .copied()
                .collect::<Vec<_>>();
            for effect_id in subscribers {
                enqueue(&mut inner, effect_id);
            }
        }
        flush_runtime(&runtime)?;
        Ok(true)
    }

    /// Write the value, panicking with a diagnostic if the runtime is invalid.
    pub fn set(&self, value: T) {
        self.try_set(value).unwrap_or_else(|error| panic!("{error}"));
    }
}

impl Effect {
    /// Dispose this effect and detach all of its subscriptions.
    pub fn dispose(&self) -> Result<(), RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        dispose_effect(&runtime, self.id)
    }
}

impl Scope {
    /// Return whether this scope has been disposed.
    pub fn is_disposed(&self) -> Result<bool, RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        let inner = runtime.borrow();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        inner.scopes.get(&self.id).map(|scope| scope.disposed).ok_or(RuntimeError::InvalidScope)
    }

    /// Create a child owner scope.
    pub fn child(&self) -> Result<Scope, RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        create_scope(&runtime, self.id)
    }

    /// Create an eager effect explicitly owned by this scope.
    pub fn effect(&self, callback: impl FnMut() + 'static) -> Result<Effect, RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        create_effect(&runtime, self.id, Box::new(callback))
    }

    /// Register cleanup to run once on disposal, in reverse registration order.
    pub fn on_cleanup(&self, callback: impl FnOnce() + 'static) -> Result<(), RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let scope = inner.scopes.get_mut(&self.id).ok_or(RuntimeError::InvalidScope)?;
        if scope.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        scope.cleanups.push(Box::new(callback));
        Ok(())
    }

    /// Dispose this scope and all resources it owns. Repeated disposal is harmless.
    pub fn dispose(&self) -> Result<(), RuntimeError> {
        let runtime = self.runtime.upgrade().ok_or(RuntimeError::RuntimeDisposed)?;
        dispose_scope(&runtime, self.id)
    }
}

fn create_scope(runtime: &RuntimeRef, parent: ScopeId) -> Result<Scope, RuntimeError> {
    let id = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let parent_node = inner.scopes.get(&parent).ok_or(RuntimeError::InvalidScope)?;
        if parent_node.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        let id = inner.next_scope;
        inner.next_scope += 1;
        inner.scopes.insert(id, ScopeNode {
            children: Vec::new(),
            effects: Vec::new(),
            cleanups: Vec::new(),
            disposed: false,
        });
        inner.scopes.get_mut(&parent).expect("parent checked").children.push(id);
        id
    };
    Ok(Scope { id, runtime: Rc::downgrade(runtime) })
}

fn create_effect(
    runtime: &RuntimeRef,
    owner: ScopeId,
    callback: Box<dyn FnMut()>,
) -> Result<Effect, RuntimeError> {
    let id = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let scope = inner.scopes.get(&owner).ok_or(RuntimeError::InvalidScope)?;
        if scope.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        let id = inner.next_effect;
        inner.next_effect += 1;
        inner.effects.insert(id, EffectNode {
            callback: Rc::new(RefCell::new(callback)),
            dependencies: BTreeSet::new(),
            owner,
        });
        inner.scopes.get_mut(&owner).expect("owner checked").effects.push(id);
        id
    };

    run_effect(runtime, id);
    flush_runtime(runtime)?;
    Ok(Effect { id, runtime: Rc::downgrade(runtime) })
}

fn track_read(runtime: &RuntimeRef, signal_id: SignalId) -> Result<(), RuntimeError> {
    let mut inner = runtime.borrow_mut();
    if inner.disposed {
        return Err(RuntimeError::RuntimeDisposed);
    }
    let Some(effect_id) = inner.active_effect else {
        return Ok(());
    };
    if !inner.effects.contains_key(&effect_id) {
        return Ok(());
    }
    let Some(signal) = inner.signals.get_mut(&signal_id) else {
        return Err(RuntimeError::InvalidSignal);
    };
    signal.subscribers.insert(effect_id);
    if let Some(effect) = inner.effects.get_mut(&effect_id) {
        effect.dependencies.insert(signal_id);
    }
    Ok(())
}

fn enqueue(inner: &mut RuntimeInner, effect_id: EffectId) {
    if inner.effects.contains_key(&effect_id) && inner.pending.insert(effect_id) {
        inner.queue.push_back(effect_id);
    }
}

fn detach_dependencies(inner: &mut RuntimeInner, effect_id: EffectId) {
    let dependencies = inner.effects.get_mut(&effect_id).map(|effect| {
        std::mem::take(&mut effect.dependencies)
    });
    if let Some(dependencies) = dependencies {
        for signal_id in dependencies {
            if let Some(signal) = inner.signals.get_mut(&signal_id) {
                signal.subscribers.remove(&effect_id);
            }
        }
    }
}

fn run_effect(runtime: &RuntimeRef, effect_id: EffectId) {
    let (callback, owner, previous_effect, previous_scope) = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed || !inner.effects.contains_key(&effect_id) {
            return;
        }
        detach_dependencies(&mut inner, effect_id);
        let effect = inner.effects.get(&effect_id).expect("effect checked");
        let callback = effect.callback.clone();
        let owner = effect.owner;
        let previous_effect = inner.active_effect;
        let previous_scope = inner.active_scope;
        inner.active_effect = Some(effect_id);
        inner.active_scope = Some(owner);
        (callback, owner, previous_effect, previous_scope)
    };
    let _ = owner;
    let result = catch_unwind(AssertUnwindSafe(|| (callback.borrow_mut())()));
    {
        let mut inner = runtime.borrow_mut();
        inner.active_effect = previous_effect;
        inner.active_scope = previous_scope;
        if result.is_err() {
            detach_dependencies(&mut inner, effect_id);
        }
    }
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}

fn flush_runtime(runtime: &RuntimeRef) -> Result<(), RuntimeError> {
    {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        // Never recursively enter the scheduler while an effect is on the stack.
        if inner.batch_depth > 0 || inner.flushing || inner.active_effect.is_some() {
            return Ok(());
        }
        inner.flushing = true;
    }

    let mut executions = 0usize;
    loop {
        let next = {
            let mut inner = runtime.borrow_mut();
            match inner.queue.pop_front() {
                Some(id) => {
                    inner.pending.remove(&id);
                    Some((id, inner.cycle_limit))
                }
                None => {
                    inner.flushing = false;
                    return Ok(());
                }
            }
        };

        let Some((effect_id, limit)) = next else { unreachable!() };
        if executions >= limit {
            let mut inner = runtime.borrow_mut();
            inner.queue.clear();
            inner.pending.clear();
            inner.flushing = false;
            return Err(RuntimeError::CycleDetected { limit });
        }
        executions += 1;

        let result = catch_unwind(AssertUnwindSafe(|| run_effect(runtime, effect_id)));
        if let Err(payload) = result {
            runtime.borrow_mut().flushing = false;
            resume_unwind(payload);
        }
    }
}

fn dispose_effect(runtime: &RuntimeRef, effect_id: EffectId) -> Result<(), RuntimeError> {
    let mut inner = runtime.borrow_mut();
    if inner.disposed {
        return Err(RuntimeError::RuntimeDisposed);
    }
    if !inner.effects.contains_key(&effect_id) {
        return Ok(());
    }
    detach_dependencies(&mut inner, effect_id);
    inner.effects.remove(&effect_id);
    inner.pending.remove(&effect_id);
    inner.queue.retain(|queued| *queued != effect_id);
    Ok(())
}

fn dispose_scope(runtime: &RuntimeRef, scope_id: ScopeId) -> Result<(), RuntimeError> {
    let (children, effects, cleanups) = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let scope = inner.scopes.get_mut(&scope_id).ok_or(RuntimeError::InvalidScope)?;
        if scope.disposed {
            return Ok(());
        }
        scope.disposed = true;
        (
            std::mem::take(&mut scope.children),
            std::mem::take(&mut scope.effects),
            std::mem::take(&mut scope.cleanups),
        )
    };

    // Descendants first, then effects in reverse creation order.
    let mut first_panic = None;
    for child in children.into_iter().rev() {
        let result = catch_unwind(AssertUnwindSafe(|| dispose_scope(runtime, child)));
        if let Err(payload) = result {
            if first_panic.is_none() { first_panic = Some(payload); }
        }
    }
    for effect in effects.into_iter().rev() {
        let result = catch_unwind(AssertUnwindSafe(|| dispose_effect(runtime, effect)));
        if let Err(payload) = result {
            if first_panic.is_none() { first_panic = Some(payload); }
        }
    }

    // Cleanup callbacks are isolated from dependency collection and always attempted.
    for cleanup in cleanups.into_iter().rev() {
        let previous_effect = {
            let mut inner = runtime.borrow_mut();
            let previous = inner.active_effect;
            inner.active_effect = None;
            previous
        };
        let result = catch_unwind(AssertUnwindSafe(cleanup));
        {
            let mut inner = runtime.borrow_mut();
            inner.active_effect = previous_effect;
        }
        if let Err(payload) = result {
            if first_panic.is_none() { first_panic = Some(payload); }
        }
    }

    if let Some(payload) = first_panic {
        resume_unwind(payload);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn only_dependent_effects_rerun() {
        let runtime = Runtime::new();
        let a = runtime.signal_eq(1).unwrap();
        let b = runtime.signal_eq(10).unwrap();
        let a_runs = Rc::new(Cell::new(0));
        let b_runs = Rc::new(Cell::new(0));
        let a_count = a_runs.clone();
        let b_count = b_runs.clone();
        let _a_effect = runtime.effect(move || { let _ = a.get(); a_count.set(a_count.get() + 1); }).unwrap();
        let _b_effect = runtime.effect(move || { let _ = b.get(); b_count.set(b_count.get() + 1); }).unwrap();

        a.set(2);
        assert_eq!(a_runs.get(), 2);
        assert_eq!(b_runs.get(), 1);
    }

    #[test]
    fn equal_writes_are_suppressed_only_for_equality_signals() {
        let runtime = Runtime::new();
        let equal = runtime.signal_eq(5).unwrap();
        let always = runtime.signal(5).unwrap();
        let equal_runs = Rc::new(Cell::new(0));
        let always_runs = Rc::new(Cell::new(0));
        let count = equal_runs.clone();
        let _e = runtime.effect(move || { let _ = equal.get(); count.set(count.get() + 1); }).unwrap();
        let count = always_runs.clone();
        let _a = runtime.effect(move || { let _ = always.get(); count.set(count.get() + 1); }).unwrap();
        equal.set(5);
        always.set(5);
        assert_eq!(equal_runs.get(), 1);
        assert_eq!(always_runs.get(), 2);
    }

    #[test]
    fn dynamic_dependencies_remove_stale_subscriptions() {
        let runtime = Runtime::new();
        let choose_a = runtime.signal_eq(true).unwrap();
        let a = runtime.signal_eq(1).unwrap();
        let b = runtime.signal_eq(10).unwrap();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let _effect = runtime.effect(move || {
            if choose_a.get() { let _ = a.get(); } else { let _ = b.get(); }
            count.set(count.get() + 1);
        }).unwrap();

        choose_a.set(false);
        a.set(2);
        assert_eq!(runs.get(), 2);
        b.set(11);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn batches_coalesce_effect_runs_and_support_nesting() {
        let runtime = Runtime::new();
        let value = runtime.signal_eq(0).unwrap();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let _effect = runtime.effect(move || { let _ = value.get(); count.set(count.get() + 1); }).unwrap();

        runtime.batch(|| {
            value.set(1);
            runtime.batch(|| value.set(2)).unwrap();
            value.set(3);
        }).unwrap();
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn scopes_dispose_effects_and_run_cleanups_lifo() {
        let runtime = Runtime::new();
        let scope = runtime.root_scope().child().unwrap();
        let value = runtime.signal_eq(0).unwrap();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let _effect = scope.effect(move || { let _ = value.get(); count.set(count.get() + 1); }).unwrap();
        let order = Rc::new(RefCell::new(Vec::new()));
        let first = order.clone();
        scope.on_cleanup(move || first.borrow_mut().push(1)).unwrap();
        let second = order.clone();
        scope.on_cleanup(move || second.borrow_mut().push(2)).unwrap();

        scope.dispose().unwrap();
        scope.dispose().unwrap();
        value.set(1);
        assert_eq!(runs.get(), 1);
        assert_eq!(*order.borrow(), vec![2, 1]);
        assert_eq!(scope.effect(|| {}).err().unwrap(), RuntimeError::DisposedScope);
    }

    #[test]
    fn cycle_guard_clears_queue_and_runtime_remains_usable() {
        let runtime = Runtime::with_cycle_limit(8);
        let value = runtime.signal(0usize).unwrap();
        let signal = value.clone();
        let effect = runtime.effect(move || signal.set(signal.get() + 1));
        assert_eq!(effect.err().unwrap(), RuntimeError::CycleDetected { limit: 8 });
        assert!(matches!(runtime.flush(), Ok(())));
        let independent = runtime.signal_eq(0).unwrap();
        independent.set(1);
        assert_eq!(independent.get(), 1);
    }

    #[test]
    fn invalid_handles_report_runtime_disposal() {
        let runtime = Runtime::new();
        let signal = runtime.signal_eq(1).unwrap();
        runtime.dispose().unwrap();
        assert_eq!(signal.try_get().unwrap_err(), RuntimeError::RuntimeDisposed);
        assert_eq!(runtime.signal_eq(2).err().unwrap(), RuntimeError::RuntimeDisposed);
    }

    #[test]
    fn nested_effects_are_owned_by_the_active_scope() {
        let runtime = Runtime::new();
        let parent_value = runtime.signal_eq(0).unwrap();
        let child_value = runtime.signal_eq(0).unwrap();
        let parent_runs = Rc::new(Cell::new(0));
        let child_runs = Rc::new(Cell::new(0));
        let p_count = parent_runs.clone();
        let c_count = child_runs.clone();
        let created = Rc::new(Cell::new(false));
        let created_flag = created.clone();
        let scope_for_nested_effect = runtime.root_scope();
        let _parent = runtime.effect(move || {
            let _ = parent_value.get();
            p_count.set(p_count.get() + 1);
            if !created_flag.replace(true) {
                let child = child_value.clone();
                let child_count = c_count.clone();
                scope_for_nested_effect.effect(move || {
                    let _ = child.get();
                    child_count.set(child_count.get() + 1);
                }).unwrap();
            }
        }).unwrap();

        parent_value.set(1);
        child_value.set(1);
        assert_eq!(parent_runs.get(), 2);
        assert_eq!(child_runs.get(), 2);
    }
}
