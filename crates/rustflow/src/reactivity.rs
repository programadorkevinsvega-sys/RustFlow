//! Single-threaded reactive runtime for RustFlow.
//!
//! The runtime uses `Rc<RefCell<_>>` intentionally: handles are neither
//! `Send` nor `Sync`. Effects run eagerly, track reads dynamically, and are
//! drained by a FIFO scheduler. Use `Runtime::batch` to coalesce writes.
//!
//! Signals created with `Runtime::signal` suppress equal writes via `PartialEq`.
//! Use `Runtime::signal_always_changed` for values without `PartialEq`.
//! Compile-fail checks document the intended single-threaded API boundary:
//!
//! ```compile_fail
//! use rustflow::reactivity::Runtime;
//! fn require_send<T: Send>() {}
//! require_send::<Runtime>();
//! ```
//!
//! ```compile_fail
//! use rustflow::reactivity::Runtime;
//! fn require_sync<T: Sync>() {}
//! require_sync::<Runtime>();
//! ```

use std::{
    any::Any,
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::{Rc, Weak},
    sync::atomic::{AtomicU32, Ordering},
};

type RuntimeRef = Rc<RefCell<RuntimeInner>>;
type SignalId = u64;
type EffectId = u64;
type ScopeId = u64;
type Cleanup = Box<dyn FnOnce()>;
type Equality<T> = Rc<dyn Fn(&T, &T) -> bool>;
type ContextId = u32;

static NEXT_CONTEXT_ID: AtomicU32 = AtomicU32::new(1);

fn next_context_id() -> ContextId {
    loop {
        let current = NEXT_CONTEXT_ID.load(Ordering::Relaxed);
        let next = current
            .checked_add(1)
            .expect("RustFlow context identity space exhausted");
        if NEXT_CONTEXT_ID
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return current;
        }
    }
}

/// Deterministic failures from invalid or disposed runtime handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    RuntimeDisposed,
    CrossRuntimeAccess,
    InvalidSignal,
    InvalidEffect,
    InvalidScope,
    DisposedScope,
    NoActiveOwner,
    DuplicateContextProvider,
    ContextTypeMismatch,
    RefDisposed,
    RefBorrowConflict,
    CycleDetected { limit: usize },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeDisposed => write!(f, "reactive runtime is disposed"),
            Self::CrossRuntimeAccess => {
                write!(f, "signal or scope belongs to a different active runtime")
            }
            Self::InvalidSignal => write!(f, "signal does not belong to a live runtime"),
            Self::InvalidEffect => write!(f, "effect is invalid or disposed"),
            Self::InvalidScope => write!(f, "owner scope is invalid"),
            Self::DisposedScope => write!(f, "owner scope is disposed"),
            Self::NoActiveOwner => {
                write!(f, "this operation requires an active component owner scope")
            }
            Self::DuplicateContextProvider => write!(f, "this owner already provides this context"),
            Self::ContextTypeMismatch => {
                write!(f, "stored context value does not match its typed identity")
            }
            Self::RefDisposed => write!(f, "owner-scoped reference is disposed"),
            Self::RefBorrowConflict => write!(f, "owner-scoped reference is already borrowed"),
            Self::CycleDetected { limit } => write!(
                f,
                "reactive cycle detected: scheduler exceeded {limit} effect executions in one flush; pending work was cleared"
            ),
        }
    }
}

impl std::error::Error for RuntimeError {}

thread_local! {
    static ACTIVE_RUNTIME_ID: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ACTIVE_OWNER_SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}
thread_local! {
    static TRACKING_PAUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct UntrackGuard(bool);
impl Drop for UntrackGuard {
    fn drop(&mut self) {
        TRACKING_PAUSED.with(|paused| paused.set(self.0));
    }
}

/// Run a closure without collecting signal dependencies into the currently active effect.
/// The previous tracking state is restored even if the closure panics.
pub fn untrack<R>(callback: impl FnOnce() -> R) -> R {
    let previous = TRACKING_PAUSED.with(|paused| paused.replace(true));
    let _guard = UntrackGuard(previous);
    callback()
}

fn runtime_id(runtime: &RuntimeRef) -> usize {
    Rc::as_ptr(runtime) as usize
}

fn check_runtime_context(runtime: &RuntimeRef) -> Result<(), RuntimeError> {
    let current = ACTIVE_RUNTIME_ID.with(std::cell::Cell::get);
    if current != 0 && current != runtime_id(runtime) {
        return Err(RuntimeError::CrossRuntimeAccess);
    }
    Ok(())
}

struct OwnerContextGuard {
    previous_scope: Option<Scope>,
    previous_runtime_id: usize,
}
impl OwnerContextGuard {
    fn enter(runtime: &RuntimeRef, owner: ScopeId) -> Self {
        let previous_scope = ACTIVE_OWNER_SCOPE.with(|active| {
            active.replace(Some(Scope {
                id: owner,
                runtime: Rc::downgrade(runtime),
            }))
        });
        let previous_runtime_id = ACTIVE_RUNTIME_ID.with(|active| {
            let previous = active.get();
            active.set(runtime_id(runtime));
            previous
        });
        Self {
            previous_scope,
            previous_runtime_id,
        }
    }
}
impl Drop for OwnerContextGuard {
    fn drop(&mut self) {
        ACTIVE_OWNER_SCOPE.with(|active| {
            active.replace(self.previous_scope.take());
        });
        ACTIVE_RUNTIME_ID.with(|active| active.set(self.previous_runtime_id));
    }
}

fn current_owner_for_runtime(runtime: &RuntimeRef) -> Option<ScopeId> {
    ACTIVE_OWNER_SCOPE.with(|active| {
        active.borrow().as_ref().and_then(|scope| {
            scope.runtime.upgrade().and_then(|owner_runtime| {
                (runtime_id(&owner_runtime) == runtime_id(runtime)).then_some(scope.id)
            })
        })
    })
}

fn active_owner_scope() -> Result<Scope, RuntimeError> {
    ACTIVE_OWNER_SCOPE
        .with(|active| active.borrow().clone())
        .ok_or(RuntimeError::NoActiveOwner)
}

/// Execute a closure with a temporary, explicitly owned component context.
/// The context is restored even if the callback panics; it is not a global runtime singleton.
pub fn with_owner_scope<R>(scope: &Scope, callback: impl FnOnce() -> R) -> Result<R, RuntimeError> {
    let runtime = scope
        .runtime
        .upgrade()
        .ok_or(RuntimeError::RuntimeDisposed)?;
    check_runtime_context(&runtime)?;
    {
        let inner = runtime.borrow();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let owner = inner
            .scopes
            .get(&scope.id)
            .ok_or(RuntimeError::InvalidScope)?;
        if owner.disposed {
            return Err(RuntimeError::DisposedScope);
        }
    }
    let _guard = OwnerContextGuard::enter(&runtime, scope.id);
    Ok(callback())
}

/// Fallibly create a signal owned by the currently active component scope.
pub fn try_signal<T: Clone + PartialEq + 'static>(value: T) -> Result<Signal<T>, RuntimeError> {
    let scope = ACTIVE_OWNER_SCOPE
        .with(|active| active.borrow().clone())
        .ok_or(RuntimeError::NoActiveOwner)?;
    scope.signal(value)
}

/// Create an owner-bound signal during component setup.
/// Calling this outside an active component owner panics with a diagnostic.
pub fn signal<T: Clone + PartialEq + 'static>(value: T) -> Signal<T> {
    try_signal(value)
        .unwrap_or_else(|error| panic!("signal requires an active component owner scope: {error}"))
}

/// Familiar component-state helper, backed by signals rather than positional hook cells.
pub fn use_state<T: Clone + PartialEq + 'static>(initial: T) -> Signal<T> {
    signal(initial)
}

/// Run a dependency-tracked effect owned by the active component scope.
pub fn use_effect(callback: impl FnMut() + 'static) -> Effect {
    let scope = ACTIVE_OWNER_SCOPE
        .with(|active| active.borrow().clone())
        .unwrap_or_else(|| panic!("use_effect requires an active component owner scope"));
    scope
        .effect(callback)
        .unwrap_or_else(|error| panic!("{error}"))
}

/// Register cleanup against the current effect run, or against its owner scope during setup.
pub fn on_cleanup(callback: impl FnOnce() + 'static) -> Result<(), RuntimeError> {
    let scope = ACTIVE_OWNER_SCOPE
        .with(|active| active.borrow().clone())
        .ok_or(RuntimeError::NoActiveOwner)?;
    let runtime = scope
        .runtime
        .upgrade()
        .ok_or(RuntimeError::RuntimeDisposed)?;
    let active_effect = { runtime.borrow().active_effect };
    match active_effect {
        Some(effect_id)
            if runtime
                .borrow()
                .effects
                .get(&effect_id)
                .is_some_and(|effect| effect.owner == scope.id) =>
        {
            scope.on_effect_cleanup(callback)
        }
        _ => scope.on_cleanup(callback),
    }
}

struct SignalNode {
    owner: ScopeId,
    subscribers: BTreeSet<EffectId>,
}

struct EffectNode {
    callback: Rc<RefCell<Box<dyn FnMut()>>>,
    dependencies: BTreeSet<SignalId>,
    cleanups: Vec<Cleanup>,
    owner: ScopeId,
}

struct ScopeNode {
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    effects: Vec<EffectId>,
    signals: Vec<SignalId>,
    contexts: HashMap<ContextId, Rc<dyn Any>>,
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

/// A read-only cached computation whose dependencies are tracked by the normal
/// owner-scoped effect scheduler.
pub struct Memo<T: Clone + 'static> {
    value: Signal<T>,
}

impl<T: Clone + 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

/// Stable mutable storage owned by a component scope and intentionally
/// excluded from the reactive dependency graph.
pub struct ScopedRef<T: 'static> {
    value: Rc<RefCell<Option<T>>>,
}

impl<T: 'static> Clone for ScopedRef<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

/// A unique typed context identity. Separate instances remain distinct even
/// when they contain values of the same Rust type.
pub struct Context<T: 'static> {
    id: ContextId,
    marker: PhantomData<fn() -> T>,
}

impl<T: 'static> Clone for Context<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            marker: PhantomData,
        }
    }
}

impl<T: 'static> fmt::Debug for Context<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context").field("id", &self.id).finish()
    }
}

impl<T: 'static> Context<T> {
    /// Create a new context identity.
    pub fn new() -> Self {
        Self {
            id: next_context_id(),
            marker: PhantomData,
        }
    }
}

impl<T: 'static> Default for Context<T> {
    fn default() -> Self {
        Self::new()
    }
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
    equality: Option<Equality<T>>,
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
                parent: None,
                children: Vec::new(),
                effects: Vec::new(),
                signals: Vec::new(),
                contexts: HashMap::new(),
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
        check_runtime_context(&self.inner)?;
        let parent = current_owner_for_runtime(&self.inner).unwrap_or_else(|| {
            let inner = self.inner.borrow();
            inner.active_scope.unwrap_or(inner.root_scope)
        });
        create_scope(&self.inner, parent)
    }

    /// Create a typed signal that suppresses writes when values compare equal.
    ///
    /// Equality follows `PartialEq`, including `NaN != NaN` for floating-point values.
    pub fn signal<T: Clone + PartialEq + 'static>(
        &self,
        value: T,
    ) -> Result<Signal<T>, RuntimeError> {
        self.create_signal(value, Some(Rc::new(|old, new| old == new)))
    }

    /// Explicitly named alias for `signal`.
    pub fn signal_eq<T: Clone + PartialEq + 'static>(
        &self,
        value: T,
    ) -> Result<Signal<T>, RuntimeError> {
        self.signal(value)
    }

    /// Create a signal for values without `PartialEq`; every write notifies subscribers.
    pub fn signal_always_changed<T: Clone + 'static>(
        &self,
        value: T,
    ) -> Result<Signal<T>, RuntimeError> {
        self.create_signal(value, None)
    }

    fn create_signal<T: Clone + 'static>(
        &self,
        value: T,
        equality: Option<Equality<T>>,
    ) -> Result<Signal<T>, RuntimeError> {
        check_runtime_context(&self.inner)?;
        let owner = current_owner_for_runtime(&self.inner).unwrap_or_else(|| {
            let inner = self.inner.borrow();
            inner.active_scope.unwrap_or(inner.root_scope)
        });
        create_signal_for_scope(&self.inner, owner, value, equality)
    }

    /// Create an eager effect owned by the active scope, or by the root scope.
    pub fn effect(&self, callback: impl FnMut() + 'static) -> Result<Effect, RuntimeError> {
        check_runtime_context(&self.inner)?;
        let owner = current_owner_for_runtime(&self.inner).unwrap_or_else(|| {
            let inner = self.inner.borrow();
            inner.active_scope.unwrap_or(inner.root_scope)
        });
        create_effect(&self.inner, owner, Box::new(callback))
    }

    /// Execute a closure as one batch. Nested batches flush only at the outer boundary.
    ///
    /// User panics are propagated after batch bookkeeping is restored. Scheduler cycle
    /// diagnostics are returned as `Err`.
    pub fn batch<R>(&self, callback: impl FnOnce() -> R) -> Result<R, RuntimeError> {
        check_runtime_context(&self.inner)?;
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
            (Ok(_value), Ok(Err(error))) => Err(error),
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
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        {
            let inner = runtime.borrow();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            let signal = inner
                .signals
                .get(&self.id)
                .ok_or(RuntimeError::InvalidSignal)?;
            let owner = inner
                .scopes
                .get(&signal.owner)
                .ok_or(RuntimeError::InvalidSignal)?;
            if owner.disposed {
                return Err(RuntimeError::DisposedScope);
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

    /// Mutate a cloned value and publish it as one write. If the callback panics,
    /// the signal retains its original value.
    pub fn update(&self, callback: impl FnOnce(&mut T)) {
        self.try_update(callback)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    /// Fallible counterpart of update. Returns true when dependent effects were notified.
    pub fn try_update(&self, callback: impl FnOnce(&mut T)) -> Result<bool, RuntimeError> {
        let mut next = self.try_get()?;
        callback(&mut next);
        self.try_set(next)
    }

    /// Write the value. Returns `true` when the value changed and `false` for
    /// equality-suppressed writes. Effects are flushed after the write completes.
    pub fn try_set(&self, value: T) -> Result<bool, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        {
            let inner = runtime.borrow();
            if inner.disposed {
                return Err(RuntimeError::RuntimeDisposed);
            }
            let signal = inner
                .signals
                .get(&self.id)
                .ok_or(RuntimeError::InvalidSignal)?;
            let owner = inner
                .scopes
                .get(&signal.owner)
                .ok_or(RuntimeError::InvalidSignal)?;
            if owner.disposed {
                return Err(RuntimeError::DisposedScope);
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
        self.try_set(value)
            .unwrap_or_else(|error| panic!("{error}"));
    }
}

impl<T: Clone + 'static> Memo<T> {
    /// Read the cached value and subscribe the current effect to memo updates.
    pub fn try_get(&self) -> Result<T, RuntimeError> {
        self.value.try_get()
    }

    pub fn get(&self) -> T {
        self.value.get()
    }
}

impl<T: 'static> ScopedRef<T> {
    /// Clone the stored value without tracking a reactive dependency.
    pub fn try_get(&self) -> Result<T, RuntimeError>
    where
        T: Clone,
    {
        let value = self
            .value
            .try_borrow()
            .map_err(|_| RuntimeError::RefBorrowConflict)?;
        value.as_ref().cloned().ok_or(RuntimeError::RefDisposed)
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.try_get().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Read through a short-lived immutable borrow without exposing a Ref guard.
    pub fn try_with<R>(&self, callback: impl FnOnce(&T) -> R) -> Result<R, RuntimeError> {
        let value = self
            .value
            .try_borrow()
            .map_err(|_| RuntimeError::RefBorrowConflict)?;
        let current = value.as_ref().ok_or(RuntimeError::RefDisposed)?;
        Ok(callback(current))
    }

    /// Mutate through a short-lived borrow. This never notifies reactive effects.
    pub fn try_with_mut<R>(&self, callback: impl FnOnce(&mut T) -> R) -> Result<R, RuntimeError> {
        let mut value = self
            .value
            .try_borrow_mut()
            .map_err(|_| RuntimeError::RefBorrowConflict)?;
        let current = value.as_mut().ok_or(RuntimeError::RefDisposed)?;
        Ok(callback(current))
    }

    /// Replace the stored value without notifying reactive subscribers.
    pub fn try_set(&self, next: T) -> Result<(), RuntimeError> {
        let mut value = self
            .value
            .try_borrow_mut()
            .map_err(|_| RuntimeError::RefBorrowConflict)?;
        let current = value.as_mut().ok_or(RuntimeError::RefDisposed)?;
        *current = next;
        Ok(())
    }

    pub fn set(&self, next: T) {
        self.try_set(next).unwrap_or_else(|error| panic!("{error}"));
    }
}

impl Effect {
    /// Dispose this effect and detach all of its subscriptions.
    pub fn dispose(&self) -> Result<(), RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        dispose_effect(&runtime, self.id)
    }
}

impl Scope {
    /// Return whether this scope has been disposed.
    pub fn is_disposed(&self) -> Result<bool, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        let inner = runtime.borrow();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        inner
            .scopes
            .get(&self.id)
            .map(|scope| scope.disposed)
            .ok_or(RuntimeError::InvalidScope)
    }

    /// Create a child owner scope.
    pub fn child(&self) -> Result<Scope, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        create_scope(&runtime, self.id)
    }

    /// Create a signal owned by this scope. The handle becomes invalid on disposal.
    pub fn signal<T: Clone + PartialEq + 'static>(
        &self,
        value: T,
    ) -> Result<Signal<T>, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        create_signal_for_scope(
            &runtime,
            self.id,
            value,
            Some(Rc::new(|old, new| old == new)),
        )
    }

    /// Create an owner-bound signal that always notifies on writes.
    pub fn signal_always_changed<T: Clone + 'static>(
        &self,
        value: T,
    ) -> Result<Signal<T>, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        create_signal_for_scope(&runtime, self.id, value, None)
    }

    /// Create an eager, owner-scoped memo.
    ///
    /// The computation runs once immediately, then reruns after scheduler flushes
    /// that invalidate dependencies read by its latest execution. Dependencies are
    /// dynamic and stale subscriptions are removed by the normal effect runner.
    /// Equal outputs by PartialEq do not notify memo consumers. Inside a batch,
    /// reads see the last committed cached value until the outer batch flushes.
    pub fn memo<T: Clone + PartialEq + 'static>(
        &self,
        compute: impl Fn() -> T + 'static,
    ) -> Result<Memo<T>, RuntimeError> {
        let cached = Rc::new(RefCell::new(None::<T>));
        let output = Rc::new(RefCell::new(None::<Signal<T>>));
        let cached_for_effect = cached.clone();
        let output_for_effect = output.clone();

        // Scope::effect performs the initial run synchronously.
        let _effect = self.effect(move || {
            let next = compute();
            let changed = cached_for_effect
                .borrow()
                .as_ref()
                .is_none_or(|previous| previous != &next);
            if changed {
                *cached_for_effect.borrow_mut() = Some(next.clone());
                if let Some(target) = output_for_effect.borrow().clone() {
                    target
                        .try_set(next)
                        .unwrap_or_else(|error| panic!("memo cache update failed: {error}"));
                }
            }
        })?;

        let initial = cached
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RuntimeError::InvalidEffect)?;
        let value = self.signal(initial)?;
        *output.borrow_mut() = Some(value.clone());
        Ok(Memo { value })
    }

    /// Create stable mutable storage owned by this scope. It is dropped on
    /// owner disposal; cloned handles then return RuntimeError::RefDisposed.
    pub fn reference<T: 'static>(&self, initial: T) -> Result<ScopedRef<T>, RuntimeError> {
        let value = Rc::new(RefCell::new(Some(initial)));
        let cleanup_value = value.clone();
        self.on_cleanup(move || {
            cleanup_value.borrow_mut().take();
        })?;
        Ok(ScopedRef { value })
    }

    /// Register a value for this context on this owner. A given owner may
    /// provide one value per context identity; descendants may shadow it.
    pub fn provide_context<T: 'static>(
        &self,
        context: &Context<T>,
        value: T,
    ) -> Result<Rc<T>, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let owner = inner
            .scopes
            .get_mut(&self.id)
            .ok_or(RuntimeError::InvalidScope)?;
        if owner.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        if owner.contexts.contains_key(&context.id) {
            return Err(RuntimeError::DuplicateContextProvider);
        }
        let shared = Rc::new(value);
        let erased: Rc<dyn Any> = shared.clone();
        owner.contexts.insert(context.id, erased);
        Ok(shared)
    }

    /// Resolve this context starting at this owner and walking toward the root.
    /// Missing providers return Ok(None). Context lookup itself is not reactive;
    /// store a Signal<T> as the value when consumers need reactive updates.
    pub fn use_context<T: 'static>(
        &self,
        context: &Context<T>,
    ) -> Result<Option<Rc<T>>, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        let inner = runtime.borrow();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let mut current = Some(self.id);
        while let Some(scope_id) = current {
            let owner = inner
                .scopes
                .get(&scope_id)
                .ok_or(RuntimeError::InvalidScope)?;
            if owner.disposed {
                return Err(RuntimeError::DisposedScope);
            }
            if let Some(value) = owner.contexts.get(&context.id) {
                return Rc::downcast::<T>(value.clone())
                    .map(Some)
                    .map_err(|_| RuntimeError::ContextTypeMismatch);
            }
            current = owner.parent;
        }
        Ok(None)
    }

    /// Create an eager effect explicitly owned by this scope.
    pub fn effect(&self, callback: impl FnMut() + 'static) -> Result<Effect, RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        create_effect(&runtime, self.id, Box::new(callback))
    }

    /// Register cleanup to run once on disposal, in reverse registration order.
    pub fn on_cleanup(&self, callback: impl FnOnce() + 'static) -> Result<(), RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let scope = inner
            .scopes
            .get_mut(&self.id)
            .ok_or(RuntimeError::InvalidScope)?;
        if scope.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        scope.cleanups.push(Box::new(callback));
        Ok(())
    }

    /// Register cleanup for the current effect run. It executes before that effect reruns
    /// and at disposal if the effect has not rerun.
    pub fn on_effect_cleanup(&self, callback: impl FnOnce() + 'static) -> Result<(), RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        check_runtime_context(&runtime)?;
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let effect_id = inner.active_effect.ok_or(RuntimeError::InvalidEffect)?;
        let effect = inner
            .effects
            .get_mut(&effect_id)
            .ok_or(RuntimeError::InvalidEffect)?;
        if effect.owner != self.id {
            return Err(RuntimeError::InvalidEffect);
        }
        effect.cleanups.push(Box::new(callback));
        Ok(())
    }

    /// Dispose this scope and all resources it owns. Repeated disposal is harmless.
    pub fn dispose(&self) -> Result<(), RuntimeError> {
        let runtime = self
            .runtime
            .upgrade()
            .ok_or(RuntimeError::RuntimeDisposed)?;
        dispose_scope(&runtime, self.id)
    }
}

fn create_signal_for_scope<T: Clone + 'static>(
    runtime: &RuntimeRef,
    owner: ScopeId,
    value: T,
    equality: Option<Equality<T>>,
) -> Result<Signal<T>, RuntimeError> {
    check_runtime_context(runtime)?;
    let id = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let owner_node = inner.scopes.get(&owner).ok_or(RuntimeError::InvalidScope)?;
        if owner_node.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        let id = inner.next_signal;
        inner.next_signal += 1;
        inner.signals.insert(
            id,
            SignalNode {
                owner,
                subscribers: BTreeSet::new(),
            },
        );
        inner
            .scopes
            .get_mut(&owner)
            .expect("owner checked")
            .signals
            .push(id);
        id
    };
    Ok(Signal {
        id,
        runtime: Rc::downgrade(runtime),
        value: Rc::new(RefCell::new(value)),
        equality,
    })
}

fn create_scope(runtime: &RuntimeRef, parent: ScopeId) -> Result<Scope, RuntimeError> {
    check_runtime_context(runtime)?;
    let id = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let parent_node = inner
            .scopes
            .get(&parent)
            .ok_or(RuntimeError::InvalidScope)?;
        if parent_node.disposed {
            return Err(RuntimeError::DisposedScope);
        }
        let id = inner.next_scope;
        inner.next_scope += 1;
        inner.scopes.insert(
            id,
            ScopeNode {
                parent: Some(parent),
                children: Vec::new(),
                effects: Vec::new(),
                signals: Vec::new(),
                contexts: HashMap::new(),
                cleanups: Vec::new(),
                disposed: false,
            },
        );
        inner
            .scopes
            .get_mut(&parent)
            .expect("parent checked")
            .children
            .push(id);
        id
    };
    Ok(Scope {
        id,
        runtime: Rc::downgrade(runtime),
    })
}

fn create_effect(
    runtime: &RuntimeRef,
    owner: ScopeId,
    callback: Box<dyn FnMut()>,
) -> Result<Effect, RuntimeError> {
    check_runtime_context(runtime)?;
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
        inner.effects.insert(
            id,
            EffectNode {
                callback: Rc::new(RefCell::new(callback)),
                dependencies: BTreeSet::new(),
                cleanups: Vec::new(),
                owner,
            },
        );
        inner
            .scopes
            .get_mut(&owner)
            .expect("owner checked")
            .effects
            .push(id);
        id
    };

    let initial_run = catch_unwind(AssertUnwindSafe(|| run_effect(runtime, id)));
    if let Err(payload) = initial_run {
        // A component/effect that panics on first execution must not remain as
        // an unreachable node in the runtime graph.
        let _ = catch_unwind(AssertUnwindSafe(|| dispose_effect(runtime, id)));
        resume_unwind(payload);
    }
    flush_runtime(runtime)?;
    Ok(Effect {
        id,
        runtime: Rc::downgrade(runtime),
    })
}

fn track_read(runtime: &RuntimeRef, signal_id: SignalId) -> Result<(), RuntimeError> {
    if TRACKING_PAUSED.with(std::cell::Cell::get) {
        return Ok(());
    }
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
    let dependencies = inner
        .effects
        .get_mut(&effect_id)
        .map(|effect| std::mem::take(&mut effect.dependencies));
    if let Some(dependencies) = dependencies {
        for signal_id in dependencies {
            if let Some(signal) = inner.signals.get_mut(&signal_id) {
                signal.subscribers.remove(&effect_id);
            }
        }
    }
}

fn run_effect(runtime: &RuntimeRef, effect_id: EffectId) {
    let (callback, owner, previous_effect, previous_scope, cleanups) = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed || !inner.effects.contains_key(&effect_id) {
            return;
        }
        detach_dependencies(&mut inner, effect_id);
        let effect = inner.effects.get_mut(&effect_id).expect("effect checked");
        let callback = effect.callback.clone();
        let owner = effect.owner;
        let cleanups = std::mem::take(&mut effect.cleanups);
        let previous_effect = inner.active_effect;
        let previous_scope = inner.active_scope;
        // Cleanup executes outside dependency collection, before the next run.
        inner.active_effect = None;
        inner.active_scope = Some(owner);
        (callback, owner, previous_effect, previous_scope, cleanups)
    };
    let owner_guard = OwnerContextGuard::enter(runtime, owner);
    let mut cleanup_panic = None;
    for cleanup in cleanups.into_iter().rev() {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(cleanup)) {
            if cleanup_panic.is_none() {
                cleanup_panic = Some(payload);
            }
        }
    }
    if let Some(payload) = cleanup_panic {
        let mut inner = runtime.borrow_mut();
        inner.active_effect = previous_effect;
        inner.active_scope = previous_scope;
        drop(inner);
        drop(owner_guard);
        resume_unwind(payload);
    }

    {
        let mut inner = runtime.borrow_mut();
        if inner.disposed || !inner.effects.contains_key(&effect_id) {
            inner.active_effect = previous_effect;
            inner.active_scope = previous_scope;
            drop(inner);
            drop(owner_guard);
            return;
        }
        inner.active_effect = Some(effect_id);
        inner.active_scope = Some(owner);
    }
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
        // A failed run must not leak cleanup resources registered before its panic.
        let failed_cleanups = {
            let mut inner = runtime.borrow_mut();
            inner
                .effects
                .get_mut(&effect_id)
                .map(|effect| std::mem::take(&mut effect.cleanups))
                .unwrap_or_default()
        };
        for cleanup in failed_cleanups.into_iter().rev() {
            let _ = catch_unwind(AssertUnwindSafe(cleanup));
        }
        drop(owner_guard);
        resume_unwind(payload);
    }
    drop(owner_guard);
}

fn flush_runtime(runtime: &RuntimeRef) -> Result<(), RuntimeError> {
    check_runtime_context(runtime)?;
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

        let Some((effect_id, limit)) = next else {
            unreachable!()
        };
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
    check_runtime_context(runtime)?;
    let (owner, cleanups) = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        if !inner.effects.contains_key(&effect_id) {
            return Ok(());
        }
        detach_dependencies(&mut inner, effect_id);
        let effect = inner.effects.remove(&effect_id).expect("effect checked");
        inner.pending.remove(&effect_id);
        inner.queue.retain(|queued| *queued != effect_id);
        (effect.owner, effect.cleanups)
    };

    let _owner_guard = OwnerContextGuard::enter(runtime, owner);
    let mut first_panic = None;
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
            if first_panic.is_none() {
                first_panic = Some(payload);
            }
        }
    }
    if let Some(payload) = first_panic {
        resume_unwind(payload);
    }
    Ok(())
}

fn dispose_scope(runtime: &RuntimeRef, scope_id: ScopeId) -> Result<(), RuntimeError> {
    check_runtime_context(runtime)?;
    let (children, effects, signals, contexts, cleanups) = {
        let mut inner = runtime.borrow_mut();
        if inner.disposed {
            return Err(RuntimeError::RuntimeDisposed);
        }
        let scope = inner
            .scopes
            .get_mut(&scope_id)
            .ok_or(RuntimeError::InvalidScope)?;
        if scope.disposed {
            return Ok(());
        }
        scope.disposed = true;
        (
            std::mem::take(&mut scope.children),
            std::mem::take(&mut scope.effects),
            std::mem::take(&mut scope.signals),
            std::mem::take(&mut scope.contexts),
            std::mem::take(&mut scope.cleanups),
        )
    };

    // Descendants first, then effects in reverse creation order.
    let mut first_panic = None;
    for child in children.into_iter().rev() {
        let result = catch_unwind(AssertUnwindSafe(|| dispose_scope(runtime, child)));
        if let Err(payload) = result {
            if first_panic.is_none() {
                first_panic = Some(payload);
            }
        }
    }
    for effect in effects.into_iter().rev() {
        let result = catch_unwind(AssertUnwindSafe(|| dispose_effect(runtime, effect)));
        if let Err(payload) = result {
            if first_panic.is_none() {
                first_panic = Some(payload);
            }
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
            if first_panic.is_none() {
                first_panic = Some(payload);
            }
        }
    }

    {
        let mut inner = runtime.borrow_mut();
        for signal_id in signals {
            if let Some(signal) = inner.signals.remove(&signal_id) {
                for effect_id in signal.subscribers {
                    if let Some(effect) = inner.effects.get_mut(&effect_id) {
                        effect.dependencies.remove(&signal_id);
                    }
                }
            }
        }
    }

    // Remove provider registrations after child owners and cleanup callbacks
    // have been disposed. Explicitly cloned Rc values remain normal Rust values.
    drop(contexts);

    if let Some(payload) = first_panic {
        resume_unwind(payload);
    }
    Ok(())
}

pub fn try_use_memo<T: Clone + PartialEq + 'static>(
    compute: impl Fn() -> T + 'static,
) -> Result<Memo<T>, RuntimeError> {
    active_owner_scope()?.memo(compute)
}

/// Create a memo during component setup; this is not a positional hook cell.
pub fn use_memo<T: Clone + PartialEq + 'static>(compute: impl Fn() -> T + 'static) -> Memo<T> {
    try_use_memo(compute)
        .unwrap_or_else(|error| panic!("use_memo requires an active owner: {error}"))
}

pub fn try_use_ref<T: 'static>(initial: T) -> Result<ScopedRef<T>, RuntimeError> {
    active_owner_scope()?.reference(initial)
}

/// Create stable non-reactive storage during component setup.
pub fn use_ref<T: 'static>(initial: T) -> ScopedRef<T> {
    try_use_ref(initial).unwrap_or_else(|error| panic!("use_ref requires an active owner: {error}"))
}

pub fn try_use_context<T: 'static>(context: &Context<T>) -> Result<Option<Rc<T>>, RuntimeError> {
    active_owner_scope()?.use_context(context)
}

pub fn try_provide_context<T: 'static>(
    context: &Context<T>,
    value: T,
) -> Result<Rc<T>, RuntimeError> {
    active_owner_scope()?.provide_context(context, value)
}

/// Register a typed value in the active component owner.
pub fn provide_context<T: 'static>(context: &Context<T>, value: T) -> Rc<T> {
    try_provide_context(context, value)
        .unwrap_or_else(|error| panic!("provide_context requires an active owner: {error}"))
}

/// Resolve the nearest provider in the active owner tree. None explicitly
/// represents a missing provider; runtime/owner failures are not defaulted.
pub fn use_context<T: 'static>(context: &Context<T>) -> Option<Rc<T>> {
    try_use_context(context).unwrap_or_else(|error| panic!("use_context failed: {error}"))
}

impl<T> rustflow_view::IntoView for Signal<T>
where
    T: Clone + rustflow_view::IntoView + 'static,
{
    fn into_view(self) -> Result<rustflow_view::View, rustflow_view::ViewError> {
        let signal = self;
        Ok(rustflow_view::View::dynamic(move || {
            signal.get().into_view()
        }))
    }
}

impl<T> rustflow_view::IntoView for Memo<T>
where
    T: Clone + PartialEq + rustflow_view::IntoView + 'static,
{
    fn into_view(self) -> Result<rustflow_view::View, rustflow_view::ViewError> {
        let memo = self;
        Ok(rustflow_view::View::dynamic(move || memo.get().into_view()))
    }
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
        let a_for_effect = a.clone();
        let b_for_effect = b.clone();
        let _a_effect = runtime
            .effect(move || {
                let _ = a_for_effect.get();
                a_count.set(a_count.get() + 1);
            })
            .unwrap();
        let _b_effect = runtime
            .effect(move || {
                let _ = b_for_effect.get();
                b_count.set(b_count.get() + 1);
            })
            .unwrap();

        a.set(2);
        assert_eq!(a_runs.get(), 2);
        assert_eq!(b_runs.get(), 1);
    }

    #[test]
    fn equal_writes_are_suppressed_only_for_equality_signals() {
        let runtime = Runtime::new();
        let equal = runtime.signal_eq(5).unwrap();
        let always = runtime.signal_always_changed(5).unwrap();
        let equal_runs = Rc::new(Cell::new(0));
        let always_runs = Rc::new(Cell::new(0));
        let equal_for_effect = equal.clone();
        let always_for_effect = always.clone();
        let count = equal_runs.clone();
        let _e = runtime
            .effect(move || {
                let _ = equal_for_effect.get();
                count.set(count.get() + 1);
            })
            .unwrap();
        let count = always_runs.clone();
        let _a = runtime
            .effect(move || {
                let _ = always_for_effect.get();
                count.set(count.get() + 1);
            })
            .unwrap();
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
        let choose_a_for_effect = choose_a.clone();
        let a_for_effect = a.clone();
        let b_for_effect = b.clone();
        let count = runs.clone();
        let _effect = runtime
            .effect(move || {
                if choose_a_for_effect.get() {
                    let _ = a_for_effect.get();
                } else {
                    let _ = b_for_effect.get();
                }
                count.set(count.get() + 1);
            })
            .unwrap();

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
        let value_for_effect = value.clone();
        let count = runs.clone();
        let _effect = runtime
            .effect(move || {
                let _ = value_for_effect.get();
                count.set(count.get() + 1);
            })
            .unwrap();

        runtime
            .batch(|| {
                value.set(1);
                runtime.batch(|| value.set(2)).unwrap();
                value.set(3);
            })
            .unwrap();
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn scopes_dispose_effects_and_run_cleanups_lifo() {
        let runtime = Runtime::new();
        let scope = runtime.root_scope().child().unwrap();
        let value = runtime.signal_eq(0).unwrap();
        let value_for_effect = value.clone();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let _effect = scope
            .effect(move || {
                let _ = value_for_effect.get();
                count.set(count.get() + 1);
            })
            .unwrap();
        let order = Rc::new(RefCell::new(Vec::new()));
        let first = order.clone();
        scope
            .on_cleanup(move || first.borrow_mut().push(1))
            .unwrap();
        let second = order.clone();
        scope
            .on_cleanup(move || second.borrow_mut().push(2))
            .unwrap();

        scope.dispose().unwrap();
        scope.dispose().unwrap();
        value.set(1);
        assert_eq!(runs.get(), 1);
        assert_eq!(*order.borrow(), vec![2, 1]);
        assert_eq!(
            scope.effect(|| {}).err().unwrap(),
            RuntimeError::DisposedScope
        );
    }

    #[test]
    fn cycle_guard_clears_queue_and_runtime_remains_usable() {
        let runtime = Runtime::with_cycle_limit(8);
        let value = runtime.signal(0usize).unwrap();
        let signal = value.clone();
        let effect = runtime.effect(move || signal.set(signal.get() + 1));
        assert_eq!(
            effect.err().unwrap(),
            RuntimeError::CycleDetected { limit: 8 }
        );
        assert!(matches!(runtime.flush(), Ok(())));
        let independent = runtime.signal_eq(0).unwrap();
        independent.set(1);
        assert_eq!(independent.get(), 1);
    }

    #[test]
    fn cross_runtime_signal_access_is_diagnosed() {
        let first = Runtime::new();
        let second = Runtime::new();
        let foreign = first.signal_eq(1).unwrap();
        let captured = foreign.clone();
        let observed = Rc::new(RefCell::new(None));
        let observed_in_effect = observed.clone();
        second
            .effect(move || {
                *observed_in_effect.borrow_mut() = captured.try_get().map(|_| ()).err();
            })
            .unwrap();
        assert_eq!(*observed.borrow(), Some(RuntimeError::CrossRuntimeAccess));
    }

    #[test]
    fn invalid_handles_report_runtime_disposal() {
        let runtime = Runtime::new();
        let signal = runtime.signal_eq(1).unwrap();
        runtime.dispose().unwrap();
        assert_eq!(signal.try_get().unwrap_err(), RuntimeError::RuntimeDisposed);
        assert_eq!(
            runtime.signal_eq(2).err().unwrap(),
            RuntimeError::RuntimeDisposed
        );
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
        let parent_value_for_effect = parent_value.clone();
        let child_value_for_effect = child_value.clone();
        let scope_for_nested_effect = runtime.root_scope();
        let _parent = runtime
            .effect(move || {
                let _ = parent_value_for_effect.get();
                p_count.set(p_count.get() + 1);
                if !created_flag.replace(true) {
                    let child = child_value_for_effect.clone();
                    let child_count = c_count.clone();
                    scope_for_nested_effect
                        .effect(move || {
                            let _ = child.get();
                            child_count.set(child_count.get() + 1);
                        })
                        .unwrap();
                }
            })
            .unwrap();

        parent_value.set(1);
        child_value.set(1);
        assert_eq!(parent_runs.get(), 2);
        assert_eq!(child_runs.get(), 2);
    }

    #[test]
    fn reentrant_writes_are_drained_iteratively() {
        let runtime = Runtime::with_cycle_limit(32);
        let value = runtime.signal_eq(0usize).unwrap();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let signal = value.clone();
        let log = observed.clone();
        let _effect = runtime
            .effect(move || {
                let current = signal.get();
                log.borrow_mut().push(current);
                if current < 5 {
                    signal.set(current + 1);
                }
            })
            .unwrap();
        assert_eq!(*observed.borrow(), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(value.get(), 5);
    }

    #[test]
    fn disposed_queued_effect_does_not_run() {
        let runtime = Runtime::new();
        let scope = runtime.root_scope().child().unwrap();
        let value = runtime.signal_eq(0).unwrap();
        let runs = Rc::new(Cell::new(0));
        let signal = value.clone();
        let count = runs.clone();
        let _effect = scope
            .effect(move || {
                let _ = signal.get();
                count.set(count.get() + 1);
            })
            .unwrap();
        runtime
            .batch(|| {
                value.set(1);
                scope.dispose().unwrap();
            })
            .unwrap();
        assert_eq!(runs.get(), 1);
        runtime.flush().unwrap();
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn effect_panic_restores_runtime_and_detaches_failed_dependencies() {
        let runtime = Runtime::new();
        let value = runtime.signal_eq(0).unwrap();
        let signal = value.clone();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = runtime.effect(move || {
                let _ = signal.get();
                panic!("intentional effect panic");
            });
        }));
        assert!(result.is_err());
        assert_eq!(value.try_set(1), Ok(true));
        assert_eq!(runtime.flush(), Ok(()));
        let independent = runtime.signal_eq(10).unwrap();
        independent.set(11);
        assert_eq!(independent.get(), 11);
    }

    #[test]
    fn cleanup_panics_do_not_skip_remaining_cleanups() {
        let runtime = Runtime::new();
        let scope = runtime.root_scope().child().unwrap();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let first_calls = calls.clone();
        scope
            .on_cleanup(move || first_calls.borrow_mut().push(1))
            .unwrap();
        let panic_calls = calls.clone();
        scope
            .on_cleanup(move || {
                panic_calls.borrow_mut().push(2);
                panic!("intentional cleanup panic");
            })
            .unwrap();
        let last_calls = calls.clone();
        scope
            .on_cleanup(move || last_calls.borrow_mut().push(3))
            .unwrap();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| scope.dispose()));
        assert!(result.is_err());
        assert_eq!(*calls.borrow(), vec![3, 2, 1]);
        assert_eq!(scope.dispose(), Ok(()));
    }

    #[test]
    fn independent_runtimes_keep_signal_values_isolated() {
        let first = Runtime::new();
        let second = Runtime::new();
        let a = first.signal_eq(1).unwrap();
        let b = second.signal_eq(10).unwrap();
        a.set(2);
        assert_eq!(a.get(), 2);
        assert_eq!(b.get(), 10);
    }

    #[test]
    fn deterministic_operation_model_matches_signal_value() {
        let runtime = Runtime::new();
        let signal = runtime.signal_eq(0_i64).unwrap();
        let mut model = 0_i64;
        for step in 0..512_i64 {
            let next = match step % 5 {
                0 => model,
                1 => model + 1,
                2 => model - 1,
                3 => step,
                _ => 0,
            };
            signal.set(next);
            model = next;
            assert_eq!(signal.get(), model, "model mismatch at step {step}");
        }
    }
    #[test]
    fn values_without_partial_eq_notify_on_every_write() {
        #[derive(Clone)]
        struct NoEq(usize);

        let runtime = Runtime::new();
        let signal = runtime.signal_always_changed(NoEq(1)).unwrap();
        let runs = Rc::new(Cell::new(0));
        let observed = signal.clone();
        let count = runs.clone();
        let _effect = runtime
            .effect(move || {
                let _ = observed.get().0;
                count.set(count.get() + 1);
            })
            .unwrap();

        assert_eq!(signal.try_set(NoEq(1)), Ok(true));
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn repeated_reads_outside_effect_do_not_subscribe_an_effect() {
        let runtime = Runtime::new();
        let outside = runtime.signal_eq(0).unwrap();
        let unrelated = runtime.signal_eq(10).unwrap();
        assert_eq!(outside.get(), 0);
        assert_eq!(outside.get(), 0);

        let runs = Rc::new(Cell::new(0));
        let observed = unrelated.clone();
        let count = runs.clone();
        let _effect = runtime
            .effect(move || {
                let _ = observed.get();
                count.set(count.get() + 1);
            })
            .unwrap();

        outside.set(1);
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn batching_preserves_fifo_order_and_deduplicates_pending_effects() {
        let runtime = Runtime::new();
        let signal = runtime.signal_eq(0).unwrap();
        let order = Rc::new(RefCell::new(Vec::new()));

        let first_signal = signal.clone();
        let first_order = order.clone();
        let _first = runtime
            .effect(move || {
                let _ = first_signal.get();
                first_order.borrow_mut().push('a');
            })
            .unwrap();
        let second_signal = signal.clone();
        let second_order = order.clone();
        let _second = runtime
            .effect(move || {
                let _ = second_signal.get();
                second_order.borrow_mut().push('b');
            })
            .unwrap();
        order.borrow_mut().clear();

        runtime
            .batch(|| {
                signal.set(1);
                signal.set(2);
                signal.set(3);
            })
            .unwrap();

        assert_eq!(*order.borrow(), vec!['a', 'b']);
    }
    #[test]
    fn signal_update_is_one_equality_aware_write() {
        let runtime = Runtime::new();
        let signal = runtime.signal(4_i32).unwrap();
        let runs = Rc::new(Cell::new(0));
        let observed = signal.clone();
        let count = runs.clone();
        let _effect = runtime
            .effect(move || {
                let _ = observed.get();
                count.set(count.get() + 1);
            })
            .unwrap();

        assert_eq!(runs.get(), 1);
        signal.update(|value| *value += 3);
        assert_eq!(signal.get(), 7);
        assert_eq!(runs.get(), 2);
        assert!(!signal.try_update(|value| *value = 7).unwrap());
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn owner_bound_signal_is_invalid_after_scope_disposal() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let signal = owner.signal(12_i32).unwrap();
        assert_eq!(signal.get(), 12);

        owner.dispose().unwrap();

        assert_eq!(signal.try_get(), Err(RuntimeError::InvalidSignal));
        assert_eq!(signal.try_set(13), Err(RuntimeError::InvalidSignal));
    }

    #[test]
    fn component_owner_context_creates_signals_and_restores_after_panic() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let scoped_signal = with_owner_scope(&owner, || signal(21_i32)).unwrap();
        assert_eq!(scoped_signal.get(), 21);
        assert!(matches!(
            try_signal(1_i32),
            Err(RuntimeError::NoActiveOwner)
        ));

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = with_owner_scope(&owner, || panic!("intentional context restoration test"));
        }));
        assert!(panic_result.is_err());
        assert!(matches!(
            try_signal(2_i32),
            Err(RuntimeError::NoActiveOwner)
        ));
        owner.dispose().unwrap();
    }

    #[test]
    fn effect_cleanup_runs_before_rerun_and_at_final_disposal() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let trigger = owner.signal(0_i32).unwrap();
        let cleanup_values = Rc::new(RefCell::new(Vec::new()));
        let observed = trigger.clone();
        let cleanup_log = cleanup_values.clone();

        let _effect = owner
            .effect(move || {
                let value = observed.get();
                let log = cleanup_log.clone();
                on_cleanup(move || log.borrow_mut().push(value)).unwrap();
            })
            .unwrap();

        trigger.set(1);
        assert_eq!(*cleanup_values.borrow(), vec![0]);
        trigger.set(2);
        assert_eq!(*cleanup_values.borrow(), vec![0, 1]);
        owner.dispose().unwrap();
        assert_eq!(*cleanup_values.borrow(), vec![0, 1, 2]);
    }

    #[test]
    fn memo_caches_initial_result_tracks_dependencies_and_suppresses_equal_outputs() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let left = owner.signal(2_i32).unwrap();
        let right = owner.signal(3_i32).unwrap();
        let runs = Rc::new(Cell::new(0));
        let left_for_memo = left.clone();
        let right_for_memo = right.clone();
        let runs_for_memo = runs.clone();
        let memo = owner
            .memo(move || {
                runs_for_memo.set(runs_for_memo.get() + 1);
                left_for_memo.get() + right_for_memo.get()
            })
            .unwrap();

        assert_eq!(memo.get(), 5);
        assert_eq!(memo.get(), 5);
        assert_eq!(runs.get(), 1);

        let observed = Rc::new(RefCell::new(Vec::new()));
        let observed_for_effect = observed.clone();
        let memo_for_effect = memo.clone();
        let _effect = owner
            .effect(move || {
                observed_for_effect.borrow_mut().push(memo_for_effect.get());
            })
            .unwrap();
        left.set(4);
        assert_eq!(memo.get(), 7);
        assert_eq!(runs.get(), 2);
        assert_eq!(*observed.borrow(), vec![5, 7]);

        // The computation reruns, but equal derived values do not notify readers.
        let parity_source = owner.signal(2_i32).unwrap();
        let parity_runs = Rc::new(Cell::new(0));
        let parity_input = parity_source.clone();
        let parity_count = parity_runs.clone();
        let parity = owner
            .memo(move || {
                parity_count.set(parity_count.get() + 1);
                parity_input.get() % 2
            })
            .unwrap();
        let parity_observed = Rc::new(Cell::new(0));
        let parity_value = parity.clone();
        let parity_calls = parity_observed.clone();
        let _parity_effect = owner
            .effect(move || {
                let _ = parity_value.get();
                parity_calls.set(parity_calls.get() + 1);
            })
            .unwrap();
        parity_source.set(4);
        assert_eq!(parity_runs.get(), 2);
        assert_eq!(parity_observed.get(), 1);
    }

    #[test]
    fn memo_dependencies_are_dynamic_and_stale_branches_are_detached() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let choose_left = owner.signal(true).unwrap();
        let left = owner.signal(1_i32).unwrap();
        let right = owner.signal(10_i32).unwrap();
        let runs = Rc::new(Cell::new(0));
        let choose = choose_left.clone();
        let left_value = left.clone();
        let right_value = right.clone();
        let run_count = runs.clone();
        let memo = owner
            .memo(move || {
                run_count.set(run_count.get() + 1);
                if choose.get() {
                    left_value.get()
                } else {
                    right_value.get()
                }
            })
            .unwrap();

        assert_eq!(memo.get(), 1);
        choose_left.set(false);
        assert_eq!(memo.get(), 10);
        left.set(2);
        assert_eq!(runs.get(), 2);
        right.set(11);
        assert_eq!(memo.get(), 11);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn memo_coalesces_batched_invalidations_and_is_disposed_with_owner() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let value = owner.signal(0_i32).unwrap();
        let runs = Rc::new(Cell::new(0));
        let source = value.clone();
        let count = runs.clone();
        let memo = owner
            .memo(move || {
                count.set(count.get() + 1);
                source.get() * 2
            })
            .unwrap();
        runtime
            .batch(|| {
                value.set(1);
                value.set(2);
                value.set(3);
                assert_eq!(memo.get(), 0);
            })
            .unwrap();
        assert_eq!(runs.get(), 2);
        assert_eq!(memo.get(), 6);
        owner.dispose().unwrap();
        assert_eq!(memo.try_get(), Err(RuntimeError::InvalidSignal));
    }

    #[test]
    fn memo_without_reactive_dependencies_runs_only_once() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let memo = owner
            .memo(move || {
                count.set(count.get() + 1);
                42_i32
            })
            .unwrap();
        assert_eq!(memo.get(), 42);
        assert_eq!(memo.get(), 42);
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn scoped_ref_is_stable_non_reactive_and_invalid_after_owner_disposal() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let reference = owner.reference(String::from("initial")).unwrap();
        let cloned = reference.clone();
        let runs = Rc::new(Cell::new(0));
        let observed = cloned.clone();
        let count = runs.clone();
        let _effect = owner
            .effect(move || {
                let _ = observed.get();
                count.set(count.get() + 1);
            })
            .unwrap();

        assert_eq!(reference.get(), "initial");
        reference.set(String::from("updated"));
        assert_eq!(cloned.get(), "updated");
        assert_eq!(
            runs.get(),
            1,
            "ref writes must not notify reactive subscribers"
        );
        reference
            .try_with_mut(|value| value.push_str("-again"))
            .unwrap();
        assert_eq!(reference.get(), "updated-again");

        owner.dispose().unwrap();
        assert_eq!(reference.try_get(), Err(RuntimeError::RefDisposed));
        assert_eq!(
            cloned.try_set(String::from("revived")),
            Err(RuntimeError::RefDisposed)
        );
    }

    #[test]
    fn typed_contexts_resolve_nearest_provider_and_distinguish_same_value_types() {
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let outer = Context::<String>::new();
        let other = Context::<String>::new();
        root.provide_context(&outer, String::from("outer")).unwrap();
        root.provide_context(&other, String::from("independent"))
            .unwrap();

        let parent = root.child().unwrap();
        let nested = parent.child().unwrap();
        parent
            .provide_context(&outer, String::from("nearest"))
            .unwrap();
        assert_eq!(
            nested
                .use_context(&outer)
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some("nearest")
        );
        assert_eq!(
            nested
                .use_context(&other)
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some("independent")
        );
        assert_eq!(
            root.use_context(&outer)
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some("outer")
        );
        assert_eq!(
            nested
                .provide_context(&outer, String::from("duplicate"))
                .unwrap_err(),
            RuntimeError::DuplicateContextProvider
        );
        assert_eq!(nested.use_context(&Context::<u8>::new()).unwrap(), None);
    }

    #[test]
    fn typed_context_values_are_isolated_per_runtime_and_signal_values_remain_reactive() {
        let context = Context::<Signal<i32>>::new();
        let first = Runtime::new();
        let second = Runtime::new();
        let first_owner = first.root_scope().child().unwrap();
        let second_owner = second.root_scope().child().unwrap();
        let first_signal = first_owner.signal(1).unwrap();
        let second_signal = second_owner.signal(10).unwrap();
        first_owner
            .provide_context(&context, first_signal.clone())
            .unwrap();
        second_owner
            .provide_context(&context, second_signal.clone())
            .unwrap();

        let received = first_owner.use_context(&context).unwrap().unwrap();
        assert_eq!(received.get(), 1);
        let values = Rc::new(RefCell::new(Vec::new()));
        let output = values.clone();
        let tracked = received.clone();
        let _effect = first_owner
            .effect(move || {
                output.borrow_mut().push(tracked.get());
            })
            .unwrap();
        first_signal.set(2);
        assert_eq!(*values.borrow(), vec![1, 2]);
        assert_eq!(
            second_owner.use_context(&context).unwrap().unwrap().get(),
            10
        );
        assert!(
            first_owner
                .use_context(&Context::<Signal<i32>>::new())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn owner_scoped_helpers_require_an_active_owner_and_context_absence_is_explicit() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let context = Context::<usize>::new();
        assert!(matches!(
            try_use_context(&context),
            Err(RuntimeError::NoActiveOwner)
        ));
        let (memo, reference) = with_owner_scope(&owner, || {
            let memo = use_memo(|| 9_usize);
            let reference = use_ref(3_usize);
            provide_context(&context, 7_usize);
            assert_eq!(use_context(&context).as_deref(), Some(&7));
            (memo, reference)
        })
        .unwrap();
        assert_eq!(memo.get(), 9);
        assert_eq!(reference.get(), 3);
        owner.dispose().unwrap();
    }

    #[test]
    fn failed_initial_memo_computation_does_not_leave_an_orphan_effect() {
        let runtime = Runtime::new();
        let owner = runtime.root_scope().child().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = owner.memo(|| -> usize { panic!("intentional memo failure") });
        }));
        assert!(result.is_err());
        assert!(runtime.inner.borrow().effects.is_empty());
        owner.dispose().unwrap();
    }
}
