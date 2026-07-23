# Contributing

This document distills a set of opinionated, battle-tested conventions for writing Rust in this project. The guidance favors code where the type system encodes intent, failures stay visible, services stay decoupled, and the public surface stays small and stable. Treat these as the default rather than suggestions. Deviate only with a clear reason. Prefer the elegant option over the expedient one, but never add complexity beyond what today's requirements actually need.

## Table of Contents

- [Architecture & API Design](#architecture--api-design)
- [Correctness & Performance](#correctness--performance)
- [Idiomatic Rust & Craft](#idiomatic-rust--craft)

---

## Architecture & API Design

These guidelines capture how we reason about module boundaries, public interfaces, and types. They are opinionated on purpose: the goal is a codebase where the type system encodes intent, services stay decoupled, and the public surface stays small and stable.

### Encapsulation and the Public API Surface

Your public API is a contract. Everything you expose becomes something a caller can depend on and something you must keep stable. Keep it small, keep it typed, and keep the machinery behind it.

#### Wrap internals in consumable types; never leak implementation types

When you hand a caller a raw internal type, you leak your implementation and freeze it in place. Expose a purpose-built type that says what the caller gets, not how you built it.

```rust
// Bad: leaks the internal storage representation.
pub fn entries(&self) -> &HashMap<u64, RawEntry> { &self.map }

// Good: a consumable view the caller can actually reason about.
pub struct EntrySummary { pub id: u64, pub label: String }
pub fn entries(&self) -> impl Iterator<Item = EntrySummary> + '_ { /* ... */ }
```

#### Don't expose runtime types in public APIs; return generic abstractions

Returning something like a channel receiver ties every caller to a specific runtime and a specific concurrency mechanism. Return a `Stream` (or another neutral abstraction) so the implementation stays yours to change.

```rust
// Bad: caller is now coupled to tokio and to "this is an mpsc".
pub fn updates(&self) -> tokio::sync::mpsc::Receiver<Update> { /* ... */ }

// Good: caller sees a stream of updates; you own the transport.
pub fn updates(&self) -> impl Stream<Item = Update> { /* ... */ }
```

Likewise, don't hand out a raw `Sender` when the caller only needs to subscribe: expose a domain method (`subscribe()`) so consumers never have to understand your internal wiring.

#### Names must reflect the interface, not the mechanism

A type named for its implementation strategy (`Arc<Mutex<_>>`, `Sink`, a serialization detail) leaks that strategy and misleads readers when it changes. Name types after their responsibility.

```rust
// Bad: "Sink" implies a shared-writer internal; "Serializer" that only maps types is misnamed.
struct EventSink { /* actually just a subscriber handle */ }

// Good: names describe role.
struct EventSubscription { /* ... */ }
struct EventConverter { /* ... */ }  // it converts, it doesn't serialize
```

#### Prefer explicit re-exports over blanket globs

`pub use module::*` re-exports whatever happens to exist today, silently widening your API as internals grow. List exports explicitly so the surface only changes when you decide it does.

```rust
// Bad: leaks every current and future public symbol of `internal`.
pub use crate::internal::*;

// Good: the surface is a deliberate, reviewable list.
pub use crate::internal::{Handler, HandlerError};
```

#### Insulate the API from internal changes with dedicated types

Reusing an internal type as your API type couples presentation to implementation; a storage change then ripples into every consumer. When in doubt, introduce a separate type at the boundary.

```rust
// Internal representation is free to evolve.
struct WidgetRecord { id: u64, blob: Vec<u8>, dirty: bool }

// Stable API type, decoupled from storage details.
pub struct Widget { pub id: u64, pub name: String }
```

### Model the Domain in the Type System

Push invariants into types so invalid states are unrepresentable and the compiler enforces your contracts. This is the highest-leverage design work you can do.

#### Prefer concrete enums over generic parameters when the set is known

If you know every variant at compile time, a generic parameter hides what's actually supported and defeats exhaustiveness checking. An enum makes the contract explicit and forces callers to handle every case.

```rust
// Bad: what types are actually valid? The signature won't say.
fn handle<T: Message>(msg: T) { /* ... */ }

// Good: the supported set is explicit and match-checked.
enum Message { Ping, Data(Vec<u8>), Close }
fn handle(msg: Message) {
    match msg { Message::Ping => {}, Message::Data(_) => {}, Message::Close => {} }
}
```

#### Use enums for mutually-exclusive state, not loose boolean flags

A bag of `bool`s admits nonsensical combinations and makes the system's state hard to read. An enum names the legal states and makes illegal ones impossible.

```rust
// Bad: is `{ active: false, draining: true }` valid? Who knows.
struct Conn { active: bool, draining: bool, closed: bool }

// Good: exactly one state, always.
enum ConnState { Active, Draining, Closed }
```

#### Wrap primitives in domain newtypes with enforced invariants

A bare `i32` or `String` carries no guarantees and mixes freely with unrelated values. A newtype that can only be constructed from a valid value makes the invariant hold everywhere it flows.

```rust
// Good: a non-negative count that cannot be constructed invalid.
pub struct ResultCount(u32);
impl ResultCount {
    pub fn new(n: i64) -> Option<Self> { u32::try_from(n).ok().map(Self) }
    pub fn get(&self) -> u32 { self.0 }
}
```

Scope such types to the domain that owns them: not every percentage in the codebase is the same `Percentage`. Also prefer specific newtypes over generic scalars in public APIs; they improve type safety and downstream tooling (e.g. generated bindings).

#### Use structs over tuples when the fields carry meaning

`((A, B), Vec<C>)` forces every reader to remember positional meaning. A named struct documents intent at the definition site.

```rust
// Bad
fn resolve() -> ((Item, Source), Vec<MissingDep>) { /* ... */ }

// Good
struct Resolution { item: Item, source: Source, missing: Vec<MissingDep> }
```

#### Encode single-writer semantics with non-cloneable types

If a resource must have exactly one writer, make the writer non-`Clone` and derive readers from it. The compiler then enforces single-producer/multi-consumer for free.

```rust
pub struct Writer { /* no Clone */ }
pub struct Reader { /* ... */ }
impl Writer {
    pub fn reader(&self) -> Reader { /* ... */ }  // many readers
}
```

### Traits, Ports, and Generics

Abstractions have a cost. Introduce a trait only when it earns its keep, define it in terms of your domain, and prefer static dispatch when you control both sides.

#### Define your own ports; don't depend on foreign traits

An abstraction boundary should expose exactly the methods your domain needs, named in your domain's terms. Depending on a foreign trait drags in unrelated methods and couples you to someone else's design.

```rust
// Good: a narrow port owned by this service.
trait WidgetStore {
    fn load(&self, id: u64) -> Option<Widget>;
    fn store(&self, w: &Widget);
}
```

Each service defines and owns its ports. Don't move a trait into a shared location just to reuse it: if two services need similar methods, the similarity is often incidental, and coupling them through one trait creates a false dependency.

#### Don't add a trait when a direct impl will do

A trait with a single implementation and no foreseeable second one is pure indirection (YAGNI). Implement methods directly on the type; extract a trait later, when a real second implementation or a real test-mock need appears.

```rust
// Bad: a trait implemented exactly once.
trait CostProvider { fn cost(&self) -> u64; }
impl CostProvider for Meter { fn cost(&self) -> u64 { self.n } }

// Good: just implement it.
impl Meter { fn cost(&self) -> u64 { self.n } }
```

#### Traits describe behavior, not implementation

A trait definition should tell readers *what* to expect, not *how* it's done. Don't leak internal tables, serialization formats, or storage details into the contract.

```rust
// Bad: bakes an encoding choice into the abstraction.
trait Codec { fn to_bytes_specific_format(&self) -> Vec<u8>; }

// Good: the contract is "encode"; the impl picks the format.
trait Codec { fn encode(&self) -> Vec<u8>; }
```

#### Prefer generics over `dyn` when the implementations are finite and you own both sides

Type erasure is for when new implementations arrive at runtime. If every implementation is known at compile time (even if the choice is made at runtime), use an enum or a generic. It's faster, it keeps exhaustiveness checks, and it's the idiomatic default here.

```rust
// Bad: erases a set that's fully known at compile time.
fn make() -> Box<dyn Decoder> { /* ... */ }

// Good: the runtime choice, statically typed.
enum AnyDecoder { Json(JsonDecoder), Binary(BinaryDecoder) }
```

Use `dyn` only when the set of implementations is genuinely open.

#### Name trait bounds for their domain role

`T: Transactionable` says almost nothing. Name the constraint after what it guarantees in your domain so `impl` blocks and signatures read as intent.

```rust
// Bad
fn commit<T: Transactionable>(store: T) { /* ... */ }

// Good
fn commit<T: AtomicStorage>(store: T) { /* ... */ }
```

Prefer domain-specific helper traits over raw foreign traits at call and mock sites. They cut boilerplate and make tests read clearly. Where a generic relationship must hold between parameters, encode it with an associated type on the trait rather than repeating bounds on every method.

### Service and Module Boundaries

Services should be autonomous, own their business logic, and communicate through narrow interfaces. Decisions belong with the code that has the context to make them.

#### Put logic where the data lives

Selection, filtering, and accounting belong inside the structure that owns the data, not scattered across callers. Callers ask a question; the owner answers it and maintains its own aggregates on mutation.

```rust
// Bad: caller reaches in and re-derives state every time.
let total: u64 = pool.items.iter().map(|i| i.weight).sum();

// Good: the structure owns the query and the aggregate.
impl Pool {
    pub fn includable(&self, max: u64) -> Vec<&Item> { /* ... */ }
    pub fn total_weight(&self) -> u64 { self.total }  // maintained on add/remove
}
```

#### Pass dependencies as parameters, not stored fields

Storing a dependency couples the holder to one version of it and blocks substitution. Passing it in at call time keeps the type flexible and testable.

```rust
// Bad: Executor is welded to one Db/Cache pair for its lifetime.
struct Executor { db: Db, cache: Cache }

// Good: upper layers supply dependencies per call.
impl Executor {
    fn run(&self, db: &Db, cache: &Cache) { /* ... */ }
}
```

Hand independent abstractions to a service separately rather than bundling them: if there are two stores, pass two stores. And avoid `static` / global state for dependencies; thread them through the call chain from context.

#### Let modules decide autonomously whether they run

A subsystem should activate based on its own configuration, not because an orchestrator flipped it on. This keeps behavior local and predictable.

```rust
impl Sync {
    fn start(cfg: &SyncConfig) -> Option<Self> {
        if cfg.peers.is_empty() { return None; } // stays off unless configured
        Some(Self::new(cfg))
    }
}
```

#### Report observations; don't make other services' decisions

Each service should surface what it sees and let downstream services judge relevance. Baking another domain's policy into your boundary leaks concerns and couples the two.

```rust
// Good: this layer reports; it doesn't decide what "good" means.
struct Observation { source: SourceId, item_seen: ItemId }
// A separate service decides whether the source is useful.
```

Give callers control over decision-making (e.g. iteration limits), and don't silently swallow edge cases at a boundary just because you won't forward them. Assign clear responsibility for handling them.

#### Structure services hierarchically with clear delegation

Prefer encapsulated services that spawn and own their sub-tasks over sprawling procedural code. This shrinks method signatures and clarifies who owns what.

```rust
impl Service for Syncer {
    fn into_task(self) -> Task {
        let importer = self.spawn_importer(); // owns its child task
        Task::new(importer)
    }
}
```

Prefer stateless computation over stateful operations where you can (e.g. deriving a value from config rather than from a specially-initialized store). It's easier to test and compose.

### Configuration

Configuration should be discoverable, composable, and honest about what is verifiable versus what is node-local.

#### Select behavior at runtime, not with compile-time feature flags

Feature-gating an implementation choice means you can't test all variants from one build and can't ship one artifact that does both. Prefer a config-driven runtime selection.

```rust
// Good: one binary, either behavior, both testable.
enum Backend { InMemory, Persistent }
fn open(kind: Backend) -> Box<dyn Store> { /* ... */ }
```

Reserve feature flags for genuinely optional code you want *out* of the binary (size/compile cost). And when you do gate on a feature, provide a separate function rather than changing an existing function's signature under `cfg`.

#### Centralize constants in one discoverable place

Magic numbers scattered across modules are impossible to tune coherently. Gather related tunables into one struct or module so behavior can be managed from a single, findable location.

```rust
pub struct Tuning { pub max_retries: u32, pub window: Duration, pub penalty: i32 }
impl Default for Tuning { /* all in one place */ }
```

#### Separate shared/verifiable config from node-specific config

Anything all participants must agree on or verify belongs in the shared config; anything local to one process's role does not. Mixing them forces node-specific fields into a contract that can't accommodate them.

```rust
// Shared, must match across participants.
struct SharedConfig { params: Params }
// Local to one process; never part of the shared contract.
struct NodeConfig { signing_key: Key, profiling: bool }
```

#### Compose configs hierarchically; never duplicate fields

If a parent config contains a child config, don't also duplicate the child's fields on the parent: they drift. Build the parent from the sub-configs.

```rust
// Bad: `retries` lives in two places and can disagree.
struct Config { retries: u32, worker: WorkerConfig /* also has retries */ }

// Good: one source of truth, decomposed into sub-configs.
struct Config { worker: WorkerConfig }
impl Config { fn worker(&self) -> &WorkerConfig { &self.worker } }
```

Place nested config where users expect it. Surprising nesting (a general concept buried under a narrow reader) is a usability bug.

#### Do process-wide setup at the binary, not the library

Initializing tracing, profiling, or other global side effects inside a library causes surprises when the library is embedded or run multiple times in one process. Keep such init at the `main` level.

```rust
// In main.rs, once per process:
fn main() { init_tracing(); init_metrics(); run(); }
// Libraries take handles/config; they don't install global state.
```

### Dependencies

Every dependency is a long-term liability: build time, artifact size, security surface, and maintenance risk.

#### Publishable crates depend only on published crates

A crate you intend to publish cannot depend on unpublished git sources. Depend on crates.io versions so the crate is actually releasable.

#### Vet new dependencies, and keep the workspace lean

Before adding a dependency, weigh its security posture, maintenance status, and its cost to `Cargo.lock` and build size against the convenience it buys. Prefer a small addition, a vendored snippet, or a sidecar tool over pulling a heavy tree into the core workspace.

#### Prefer mature libraries over bespoke implementations

For well-trodden problems (caches, LRUs, data structures), a battle-tested crate usually beats a hand-rolled version on both correctness and performance. Reach for the library first.

### Reuse, Resources, and Reliability

#### Extract repeating patterns into named helpers or extension traits

When the same shape appears in several spots, give it a name. An extension trait or helper method makes the intent explicit and connects the call sites so they evolve together.

```rust
trait ResultExt<T> {
    fn or_continue(self) -> ControlFlow<(), T>;
}
// Now call sites say what they mean instead of repeating the match.
```

#### Apply the Law of Demeter: prefer named operations over method chains

A long chain reaching through several objects couples the caller to a deep structure. Extract each logical step into a named method on the owner.

```rust
// Bad
self.state.storage.view.flush();

// Good
self.flush_view();  // and self.flush_storage(), etc.
```

#### Use RAII / `Drop` for cleanup instead of manual rollback

Tie resource lifecycle to a value's scope so cleanup happens automatically, even on early return or panic. This is simpler and safer than hand-written teardown paths.

```rust
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}
```

#### Prefer idempotent, retryable operations over state recovery

Don't build fragile machinery to recover partial state when the operation could simply be safe to re-run. An idempotent operation that redoes lost work is far more robust.

```rust
// Re-running is harmless; missed work is simply redone next pass.
fn apply_migration(&self, m: &Migration) { if !self.already_applied(m) { m.run(); } }
```

#### Use one shared registry for cross-cutting resources

Creating a registry (metrics, and similar) per module fragments what should be global. Register into a single shared instance so everything is collected in one place.

```rust
// Bad: each module makes its own.
let reg = Registry::new();

// Good: register into the process-wide one.
fn register(global: &Registry) { global.register(counter()); }
```

#### Assign stable integer IDs to schema columns

Stable IDs let you remove or reorder columns later without breaking existing data. Bind each column to an explicit number rather than relying on position or name.

```rust
#[repr(u32)]
enum Column { Id = 0, Name = 1, Payload = 2 }  // 1 can be retired later, safely
```

When you introduce a new resource of an existing kind, update *every* code path that already handles that kind: half-integrated resources are a reliable source of bugs.

### Testing and Observability

#### Scope test-only surface to where it's used

Test scaffolding dedicated to one module shouldn't be publicly reachable. Keep it private to that module, or gate it behind a `test-helpers` feature so the real public API stays clean.

```rust
#[cfg(any(test, feature = "test-helpers"))]
impl Cache { pub fn assert_integrity(&self) { /* ... */ } }
```

Where a test needs to assert on internal state, exposing the real field (behind a test gate) is often cleaner than bending the production trait around test needs.

#### Put metrics where the domain context lives

Instrument the component that holds the most context about the operation, so the metric carries meaning. For long-lived background work, track lifecycle with an RAII guard that increments on start and decrements on exit. The count stays correct even if the task ends abnormally.

```rust
struct ActiveGuard(&'static Gauge);
impl ActiveGuard { fn new(g: &'static Gauge) -> Self { g.inc(); Self(g) } }
impl Drop for ActiveGuard { fn drop(&mut self) { self.0.dec(); } }
```

---

## Correctness & Performance

These guidelines capture patterns that recur in review. They favor making illegal states unrepresentable, keeping failures visible, and paying for work only where it is genuinely needed.

### Error Handling & Failure Modes

#### Reserve panics for truly unrecoverable state

`panic!`, `unwrap()`, and `expect()` halt everything. Use them only when continuing would mean operating on corrupt data: a poisoned lock, a violated invariant that indicates a logic bug. For anything a caller could reasonably recover from, return a `Result` so the rest of the system can degrade gracefully instead of crashing. An `unwrap()` on I/O can take down the whole process over a spurious error.

```rust
// Bad: a transient failure crashes the process
let value = cache.load(&key).unwrap();

// Good: propagate recoverable failures
let value = cache.load(&key)?;

// Justified panic: the lock is poisoned, state is unrecoverable
let guard = shared.lock().expect("Cache mutex poisoned: state is corrupt");
```

Beware shared accessors that can panic under concurrency: `Arc::try_unwrap` panics if other clones exist, so never expose it on an API several callers might hit at once.

#### Prefer `expect()` with a reason over `unwrap()` or `?`

When an invariant guarantees a value is present, `expect()` documents *why* it must exist and reads better than threading a `Result` through code that can never actually fail. Don't use `?` to quietly paper over a missing value that should always be there. That hides a real bug.

```rust
// A validated Config guarantees this field; document the invariant.
let port = config
    .port
    .expect("Config validated at construction always sets `port`");
```

Where an algorithm's own structure guarantees success, prefer making the function **infallible** and proving it with tests, rather than adding a `Result` variant that can never be constructed.

#### Use `Result` and `Option` for their actual meanings

Return `Result` when a boolean or sentinel would discard information the caller needs to act on. Reserve error types for genuinely *unexpected* conditions; use `Option` for data that is legitimately, expectedly absent. And don't conflate "empty" with "missing."

```rust
// Bad: caller can't tell why the check failed
fn validate(w: &Widget) -> bool;

// Good: the error carries meaning
fn validate(w: &Widget) -> Result<(), ValidationError>;

// Option for expected absence; empty != nonexistent
fn lookup(&self, id: WidgetId) -> Option<Vec<Entry>>;
```

Use `TryFrom` rather than `From` for conversions that can fail, so fallibility is explicit instead of hidden behind a panic.

#### Never swallow errors silently

Returning a default in place of an error hides failures until they surface as something worse downstream. At minimum, log unexpected errors, even when you recover, and emit an error trace for failures you tolerate, so operators can *see* the system struggling. Silently discarding data that feeds shared state can cause divergence between nodes that should agree.

```rust
match store.read(&key) {
    Ok(v) => v,
    // Bad: return Default::default()
    Err(e) => {
        tracing::error!(?e, "read failed for {key:?}");
        return Err(e.into());
    }
}
```

For "impossible" branches, return an explicit error and log that the path should be unreachable; in code where the invariant should hold, a `debug_assert!(false)` plus a `warn!` gives both a test-time trip wire and production visibility.

#### Preserve error context across boundaries

When wrapping errors between layers, keep the source chain intact instead of flattening it to a string.

```rust
#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("config missing")]
    Config,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

Prefer concrete payloads over unit types in error contexts (`Option<SignalKind>` over `Option<()>`) so diagnostics carry the useful value. For evolving protocols, use error *codes* and handle unknown codes gracefully so new variants don't break old peers.

#### Use checked arithmetic where overflow signals a bug

Don't trust type-level bounds to prevent overflow. Reach for `checked_*` when an overflow would indicate a logic error you want surfaced, not `saturating_*`, which silently clamps and masks it.

```rust
let next = counter
    .checked_add(1)
    .ok_or(HandlerError::CounterOverflow)?;
```

In systems requiring reproducibility, prefer integer math: float arithmetic is not deterministic across platforms.

### Concurrency & Async

#### Match the synchronization primitive to the semantics

A `Mutex` guards *access* to a resource; a `Semaphore` *signals* availability or that work is in progress. Use `RwLock` for read-heavy data to allow concurrent readers, and prefer a plain atomic when the state is a single counter or flag. Reach for the simplest tool that fits.

```rust
// Read-mostly shared config: readers don't block each other
let config: RwLock<Config> = RwLock::new(Config::default());

// A simple flag: an atomic beats a Mutex<bool>
let ready = AtomicBool::new(false);
```

Never put a coarse `Mutex` on top of a lock-free structure like `DashMap`: it serializes everything and erases the concurrency you paid for.

#### Share owned state with `Arc`, and let the last owner clean up

When a value is cloned across threads, wrap it in `Arc` (and `Arc<Mutex<T>>` / `Arc<RwLock<T>>` for shared mutation) so clones are cheap and safe. For clonable handles that own a resource, ensure `Drop` releases the resource only when the *last* clone goes away. Otherwise one dropped handle tears down state others still use.

```rust
#[derive(Clone)]
struct Handle {
    inner: Arc<Resource>, // resource freed only when the last Handle drops
}
```

Never return a reference borrowed from a `MutexGuard`: once the guard releases, the reference dangles. Return an owned clone or run the work while the guard is held.

#### Keep blocking work off the async runtime

A blocking call on an async worker stalls every task sharing that thread. Wrap synchronous CPU- or I/O-bound work in `spawn_blocking`, and move genuinely long-running jobs into their own task that communicates by channel.

```rust
let result = tokio::task::spawn_blocking(move || heavy_compute(&input)).await?;
```

Avoid `block_on`: it is a code smell for an architectural mismatch and a frequent source of deadlocks. And don't make constructors `async`: keep initialization synchronous and defer address resolution or connection setup to a `start()` method.

#### Only mark things `async` when they actually await

A function that returns immediately should be `sync`. Classify an operation as sync or async by its real computational model, not for surface consistency: work that fully consumes a thread and cannot be parallelized belongs on a blocking path, not dressed up as a future.

#### Prefer channels to shared locks

Message passing gives you backpressure for free and avoids holding multiple locks at once. When several components contend on shared mutable state, a channel is often clearer and less deadlock-prone than a web of mutexes.

```rust
let (tx, mut rx) = tokio::sync::mpsc::channel(64);
tokio::spawn(async move {
    while let Some(cmd) = rx.recv().await { handler.apply(cmd); }
});
```

#### Know your task and lock lifecycles

Dropping a `JoinHandle` **detaches** the task: it keeps running orphaned rather than being cancelled. Hold the handle (or an explicit cancellation guard) for the task's intended lifetime. Understand your lock ordering to avoid deadlocks: taking a second write lock while already holding one on the same structure will hang.

#### Re-check state after awaiting

Anything you awaited on may have changed by the time you wake. Snapshot or clone the state you need *before* the await to avoid a race between the check and the wait, refresh stale views once you resume, and add a timeout so a wait can't hang forever.

```rust
let target = *self.desired.borrow(); // snapshot before awaiting
self.notified().await;
// re-read the view; it may have advanced past `target`
```

Validate inputs *before* taking a lock, so bad data doesn't cause you to lock, do nothing, and unlock under contention.

### State, Invariants & Data Integrity

#### Make invalid states unrepresentable

Enforce invariants at construction and in the type system rather than re-checking at runtime. If a value must be read-only, encode that in its type; if certain elements must never enter a collection, don't insert them in the first place instead of filtering later.

```rust
// A read-only view can't accidentally mutate, enforced by the type
struct ReadView<'a>(&'a Store);

// Don't add ineligible entries, then guard against them everywhere after
if entry.is_eligible() { creators.insert(entry); }
```

Use distinct types for semantically different values (an internal index vs. an external one) so they can't be swapped by mistake, and normalize public API integers to `u64` rather than platform-dependent `usize`.

#### Order mutations so observers never see torn state

Write the primary record before updating any secondary index, so a concurrent reader that hits the index always finds the backing data. Clear or advance shared buffers only *after* a commit succeeds, so an error can't lose data. Document any multi-step operation that is not atomic: a future maintainer needs to know a concurrent write can corrupt the result.

```rust
store.insert(id, record)?; // primary first
index.insert(key, id)?;    // then the pointer to it
```

#### Make retryable operations idempotent

Anything that may be retried or replayed must converge to the same result. Deduplicate on a stable identifier and skip work already done, so a retry can't double-apply an effect. When you validate an externally-provided structure, reproduce it and compare rather than trusting the caller's fields. And reject stale or out-of-range inputs *before* mutating anything.

```rust
if seen.contains(&op.id) { return Ok(()); } // already applied
apply(&op)?;
seen.insert(op.id);
```

Use the *entire* payload when checking uniqueness: a partial hash invites collisions that silently drop distinct items. Validate untrusted external input before acting on it, and check critical preconditions before expensive work.

#### Order validation cheap-first

Run the inexpensive checks before the costly ones so invalid input bails out early.

```rust
check_bounds(&req)?;   // cheap
verify_signature(&req)?; // expensive, only if bounds passed
```

Reuse validation you've already done rather than repeating it: if a checked type already guarantees a property, don't re-verify it, and if one computed set already contains all the cases, a length check may cover them all.

#### Keep recovery symmetric with runtime

Startup, restart, and rollback paths must mirror the forward path exactly. If runtime sets a field, recovery must restore it, not leave it `None`. If a forward step derives extra state, rollback must clean *all* of it, not just the parts reachable through one accessor. Producer and validator paths should be tested symmetrically: whatever you produce must pass its own validation.

### Performance & Resource Use

#### Iterate and stream lazily instead of materializing

Collecting a large dataset into a `Vec` just to fold or re-iterate it wastes memory. Chain iterator operations so items are produced and discarded one at a time, and lean on iterator combinators: the compiler optimizes them more aggressively than hand-rolled loops.

```rust
// Bad: loads everything, then reduces
let total: u64 = store.load_all()?.into_iter().map(|w| w.size).sum();

// Good: stream and discard as you go
let total: u64 = store.iter()?.map(|w| w.size).sum();
```

Don't `collect()` only to call `.iter()` on the result moments later; pass the iterator through. For small datasets already in memory, skip cooperative `yield` points: they add overhead with nothing to gain.

#### Batch, and yield between batches for large work

Prefer a single multi-get or a combined query over N individual lookups, especially when each call takes a lock. When a job is genuinely large, process it in chunks and yield between them so other tasks make progress.

```rust
// Bad: N round-trips, N lock acquisitions
for k in &keys { results.push(store.get(k)?); }

// Good: one batched request
let results = store.multi_get(&keys)?;
```

Cache expensive computations at the call site instead of recomputing them across the codebase, and reuse a value already in scope rather than recalculating it. Where a single reference streams to a target while keeping memory flat, prefer the database's native backward pagination over loading everything and calling `.rev()`.

#### Choose algorithms and data structures for the access pattern

Avoid `O(n)` scans where an `O(1)` lookup fits, and never nest a `find()` inside a loop for `O(n²)` behavior when one pass with a break suffices. Use the `entry` API to avoid probing a map twice, and hoist invariant conditionals out of loops so the branch isn't re-evaluated every iteration.

```rust
// Bad: two lookups
if !map.contains_key(&k) { map.insert(k, mk_default()); }
let slot = map.get_mut(&k).unwrap();

// Good: one lookup, and no eager allocation
let slot = map.entry(k).or_insert_with(mk_default);
```

Prefer LRU eviction over periodically wiping a whole cache, and avoid deep recursion that can overflow the stack. Drive it with an explicit queue or channel instead.

#### Avoid needless clones and allocations in hot paths

Cloning a large structure per iteration is ruinous at scale. Share it via `Arc`, or thread a borrow through (`&self`, `&[T]`) instead of taking ownership. Skip intermediate `to_vec()` / `collect()` steps, and don't build a closure that clones when a direct call or reference works.

```rust
// Bad: clone a big value into every task
for task in &tasks { spawn(state.clone(), task); }

// Good: share ownership
let state = Arc::new(state);
for task in &tasks { spawn(Arc::clone(&state), task); }
```

Watch APIs with hidden costs: `entry(..).or_default()` allocates a value even when unused, and deserializing outputs you never read is pure waste: skip it. In hot paths that recompute a derived value on every setter call, a builder that computes once at the end is cheaper. Keep hot-path and event-loop functions lightweight, pushing non-critical work to insert/remove handlers or background tasks.

### Observability & Documentation

#### Instrument for what you need to see

Track skipped and failed items alongside successes so the picture is complete, not just the happy path. Choose metric shapes deliberately: a histogram is for variable distributions like latency; a constant or slowly-changing property belongs in a gauge. Optimize notification patterns to avoid excessive wakeups under high throughput: a targeted subscriber list beats waking everyone.

```rust
counter!("items_processed").increment(ok as u64);
counter!("items_skipped").increment(skipped as u64); // don't drop these
histogram!("handler_latency_seconds").record(elapsed);
```

#### Document invariants, hazards, and unsafe code

Leave notes for the next maintainer where behavior rests on an assumption that could change, where an operation is deliberately non-atomic, or where `unsafe` is used: explain *why* it is sound and what precaution upholds that. These comments are how future changes avoid quietly breaking a load-bearing assumption.

```rust
// SAFETY: `idx < self.len` is guaranteed by the bounds check above,
// so this read can never be out of range.
unsafe { self.buf.get_unchecked(idx) }
```

#### Test the logic and the edges

Trait implementations that carry real logic deserve tests that verify they uphold their contract, not just that they compile. Cover boundary conditions explicitly (the first and last element of a range, off-by-one prefix iteration, a zero or empty input) and write adapter-level tests that deterministically reproduce a real production failure, so a fix is provably a fix.

#### Prefer focused designs and safe defaults

An abstraction that tries to solve every problem is a design smell; favor single-responsibility components. Make dangerous behaviors configurable and default to the safe choice in production. For fragile structured formats like URLs, use a purpose-built library rather than hand-rolled string concatenation, and gate breaking changes behind feature flags to preserve backwards compatibility while you migrate.

---

## Idiomatic Rust & Craft

These conventions favor code that reads declaratively, encodes invariants in types, and stays cheap to refactor. Prefer the elegant option over the expedient one, but do not add complexity beyond what the current requirements need.

### Naming

**Name things after what they *are* or *do*, not how they're built.** A reader should understand intent from the name alone, without opening the body or reading a comment. Generic verbs (`block`, `process`), implementation hints, and version suffixes (`v2`) age badly; behavior-based names do not.

```rust
// Bad: vague, or leaks a detail, or needs a comment to explain the return
fn block(w: &Widget) { /* ... */ }
fn extra_checks(cfg: &Config) -> bool { /* ... */ }

// Good: the name is the documentation
fn store_widget_if_new(w: &Widget) -> Stored { /* ... */ }
fn acquire_leases_on_unowned_nodes(&self) { /* ... */ }
```

**Let names track semantics precisely.** A `maybe_` prefix implies `Option`, not `Result`; a control-flow type is a `State`/`Action`/`Transition`, not a `Result`; a function that "removes" should not also add. When the name and the behavior drift apart, rename one of them.

```rust
// `maybe_` implies Option: use a name that matches a Result
let widget_res: Result<Widget, Error> = build();

// Model control flow as what it is, not as success/failure
enum TaskState { Continue, Yield, Done }
```

**Follow `UpperCamelCase` conventions for acronyms: treat them as one word.** Write `Uuid`, `HttpClient`, `P2p`, not `UUID`, `HTTPClient`, `P2P`. This keeps word boundaries unambiguous.

**Name errors `Error` inside their module; alias on import.** A module already qualifies the type (`config::Error`), so an internal `ConfigError` is redundant. When a caller pulls in several, alias at the import site.

```rust
// in config.rs
pub enum Error { /* ... */ }

// at a call site that needs two
use config::Error as ConfigError;
use store::Error as StoreError;
```

**Name modules by responsibility, not mechanism.** `old_widget_source.rs` tells a reader what lives there; `loader_and_cache_impl.rs` does not. Put code in the section or module where its concern belongs.

### Types & API Design

**Encode invariants in the type system with newtypes and enums.** A newtype makes invalid values unconstructible and documents guarantees at the boundary; an enum makes illegal states unrepresentable where a bare `bool` or pair of `Option`s cannot. Prefer these over runtime checks and boolean flags.

```rust
// A value that is clamped by construction: callers never re-validate
pub struct ClampedPercent(u8);
impl ClampedPercent {
    pub fn new(v: u8) -> Self { Self(v.min(100)) }
}

// Replace a bool flag with an enum that names the states
enum Retry { Enabled, Disabled }   // not: run(should_retry: bool)
```

**Prefer static dispatch and borrowed inputs in signatures.** Take `impl Trait` / trait bounds instead of `Box<dyn Trait>` where a blanket impl already applies, accept `&[T]` instead of `&Vec<T>`, and take `&T` instead of an owned `T` when you don't need ownership. This avoids allocation and indirection and accepts more call sites.

```rust
// Bad
fn run(h: Box<dyn Handler>, items: &Vec<Item>, cfg: Config) { /* ... */ }

// Good: monomorphizes, accepts arrays/slices, borrows what it only reads
fn run(h: impl Handler, items: &[Item], cfg: &Config) { /* ... */ }
```

**Return `impl Iterator`; don't `collect()` prematurely.** Materializing into a `Vec` (then sometimes boxing it) forces an allocation and discards laziness. Hand back an iterator and let the caller decide whether to collect. Watch for double-wrapping: don't call `.into_boxed()` on something already boxed.

```rust
// Bad: allocates a Vec the caller may never fully consume
fn ids(&self) -> Vec<Id> { self.items.iter().map(|i| i.id).collect() }

// Good
fn ids(&self) -> impl Iterator<Item = Id> + '_ {
    self.items.iter().map(|i| i.id)
}
```

**Avoid redundant and anti-pattern wrappers.** `Arc<Box<dyn T>>` double-allocates: `Arc<dyn T>` gives the same indirection. `Arc<&T>` is a smell: accept `impl Borrow<T>` so owned and borrowed callers both work. Don't reach for `Deref` to convert types; use explicit `AsRef`/`From` so the conversion is visible.

```rust
fn store(&self, item: impl Borrow<Item>) {
    let item = item.borrow();
    // works whether the caller holds an Item, &Item, or Arc<Item>
}
```

**Group related parameters into a struct.** When a function needs two or more fields that travel together (or you can foresee more arguments arriving), pass a config/parameter struct. It stops signatures from growing and keeps call sites readable.

```rust
// Bad: every new knob is a breaking signature change
fn open(path: &Path, cache_mb: usize, read_only: bool, create: bool) { /* ... */ }

// Good
struct OpenOptions { cache_mb: usize, read_only: bool, create: bool }
fn open(path: &Path, opts: &OpenOptions) { /* ... */ }
```

**Prefer named struct fields over tuple indices.** `.widget_db` / `.widget_events` cannot be silently swapped the way `.0` / `.1` can, and they read without a legend.

**Centralize conversions in `From`/`TryFrom` and hide surplus generics.** Implement the standard conversion traits instead of scattering ad-hoc `foo_from_bar` helpers, and prefer working through references in those impls. Too many type parameters leak into an API and make it hard to use. Reach for a concrete type when the flexibility isn't needed.

**Reuse existing abstractions instead of hand-rolling.** Use `size_of::<T>()` over `BITS / 8`, derive standard traits (e.g. via `derive_more`) instead of writing `Add`/`Sub` by hand, use a real encoder rather than bespoke byte-shuffling, and reach for `serde`'s `is_human_readable()` to support both text and binary formats from one impl.

```rust
let n = size_of::<u64>();            // not: (u64::BITS / 8) as usize

#[derive(derive_more::Add, derive_more::Sub)]
struct Amount(u64);
```

### Error Handling

**Return specific error types from libraries; reserve `anyhow` for binaries.** Library callers need to match on and react to failures: an opaque `anyhow::Error` takes that control away. Define domain-specific error enums (encoding errors, config errors) at your boundaries. `anyhow`'s `.context()` is still the right tool in application/binary code for user-facing troubleshooting.

```rust
// Library: give callers something to match on
#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("key not found")] NotFound,
    #[error("backend unavailable")] Unavailable,
}

// Binary: enrich with context for the end user
let db = open(&path)
    .with_context(|| format!("failed to open database at {}", path.display()))?;
```

### Functions & Control Flow

**Break long or deeply nested functions into small, named helpers.** Sub-functions with descriptive names turn a wall of logic into a checklist a reader can skim, and each unit becomes independently testable. Extracting a `validate_config` reads its requirements line-by-line even when each check is trivial.

```rust
fn admit(req: &Request) -> Result<(), Error> {
    check_size(req)?;
    check_rate_limit(req)?;
    check_permissions(req)?;
    Ok(())
}
```

**Favor combinators and iterators over manual branching and counters.** Use `if let` instead of `.is_some()` + `.unwrap()`, `map` instead of an if/else that calls the same function two ways, OR-patterns to group match arms, and iterator predicates (`.filter(..).count()`, `.any(..)`) instead of a hand-maintained counter.

```rust
// Bad
if opt.is_some() { use_it(opt.unwrap()); }
// Good
if let Some(v) = opt { use_it(v); }

// Group related arms
match (a, b, c, d) {
    (Some(_), Some(_), _, _) | (_, _, Some(_), Some(_)) => merge(),
    _ => skip(),
}
```

**Bind intermediate values instead of nesting calls, and don't couple unrelated ideas in one expression.** Naming the middle of a chain gives a reader a foothold and a place to breakpoint.

```rust
// Bad
handle(build(ConnectionArgs::from(request)));
// Good
let args = ConnectionArgs::from(request);
let conn = build(args);
handle(conn);
```

**Remove unused parameters and fields.** A parameter that's threaded through but never used is a trap that invites incorrect calls; delete it.

**Eliminate redundant state and duplicated logic.** If one condition already implies another (a `Some` registry *means* metrics are on), don't carry a separate boolean. Refactor shared behavior into one place rather than copying it: duplication is where future refactors diverge and rot.

### Tests

**Name tests after the requirement, including the system under test.** The name should state condition and expected outcome so a failure report is self-explanatory: `insert_with_missing_input_widget_fails`, not `test_insert_2`. Many maintainers favor an explicit given/when/then shape. Include the SUT so it's clear what's exercised, and flag the test kind (regression, snapshot) when relevant.

```rust
#[test]
fn cache_returns_stale_entry_when_refresh_fails() { /* given / when / then */ }

// For table tests, document each case in human-readable prose:
#[test_case(Config::empty() ; "returns default when config is empty")]
```

**Test observable behavior through the public interface, not internals.** A test that only proves code ran, one you could satisfy with a no-op implementation, verifies nothing; assert on resulting state and side effects. Testing private functions and internal APIs couples tests to implementation and makes them brittle. (Reach past the public boundary only when that boundary genuinely doesn't exist yet, and treat that as a smell to fix.)

```rust
// Weak: passes even if set_limit does nothing
set_limit(&mut cache, 10);

// Strong: assert the effect
set_limit(&mut cache, 10);
assert_eq!(cache.limit(), 10);
```

**Prefer fakes and manual test doubles over procedural mocks.** Call a working stand-in a `Fake`, not a `Mock`: reserve "mock" for true behavior verification. Auto-generated mocks tempt you to over-constrain call counts and assert on *how* code works instead of *what* it produces; a hand-written fake keeps tests aimed at the contract.

**Make tests reproducible, isolated, and self-contained.** Seed RNGs so failures replay. Avoid global/shared state and cross-test coupling that causes flakiness (prefer injected dependencies over ambient globals). Don't depend on external URLs: provide an offline mode or a local double.

```rust
let mut rng = StdRng::seed_from_u64(0xC0FFEE); // reproducible failures
```

**Keep tests short, focused, and self-documenting; add helpers and edge cases.** Ten small tests differing by one named variable carry less cognitive load than one sprawling test. Let declarative helper names replace comments. Cover boundaries and the negative space, not just the happy path. And for a value range, consider `proptest` over hand-picked points. Make defaults explicit in tests so a changed default surfaces as a failure.

```rust
proptest! {
    #[test]
    fn clamp_never_exceeds_max(v in any::<u8>()) {
        prop_assert!(ClampedPercent::new(v).get() <= 100);
    }
}
```

**Require integration tests for new features, and mark expensive tests.** A unit test that a flag exists isn't proof the feature works end-to-end; add an integration test that a config change actually changes behavior. Tag slow tests in the runner config so they don't stretch the normal cycle.

### Dependencies, Tooling & Hygiene

**Use modern module and macro style.** Prefer `foo.rs` over `foo/mod.rs` (Rust 2015 style), and import macros with normal `use` rather than `#[macro_use]` (stable since Rust 1.30).

**Avoid patch-level version pins in library dependencies.** Different patch requirements across libraries collide when combined downstream. Commit `Cargo.lock` for binaries, though: it's what makes builds reproducible and CI caching effective.

**Be explicit about lints and log levels.** Spell out enforced `deny`/`allow` attributes, and use `const` for compile-time math so clippy can reason about it (rather than silencing `arithmetic_side_effects`). Log user-facing state changes at `info`; keep high-frequency internal events at `debug`/`trace` so production logs don't flood. Log related events on one line to stay coherent under concurrency.

```rust
const HEADER_LEN: usize = size_of::<u32>() * 2; // const, so no arithmetic-lint suppression
debug!("cache miss for key {key}");            // not info! on a hot path
```

**Replace magic values with named (and documented) constants, used consistently.** A named constant is self-documenting and refactor-safe; a comment explaining *why* a value must hold (e.g. one bound must exceed another) stops a future edit from breaking an unstated invariant. Reference the same constant everywhere rather than repeating a literal or string.

```rust
/// Must exceed RESERVED_SLOTS so background work always has headroom.
const POOL_SIZE: usize = 64;
const CONFIG_FILENAME: &str = "config.toml"; // one definition, referenced everywhere
```

**Comment the *why*, but let code carry the *what*.** Add comments for non-obvious API calls and for the reasoning behind a design choice; don't narrate what well-named code already says. Don't strip comments that explain genuinely non-obvious behavior just for brevity, and keep comments in sync with any generated documentation.

**Keep commits and constructors focused and honest.** Revert unrelated formatting or indentation churn so a diff shows only its intent. Prefer explicit constructors that set static values over a `Default` impl when the initialization intent should be visible, and don't lean on implicit meanings of defaults (spell out what a default value signifies rather than assuming a reader knows).

```rust
// Bad: what does Default even mean here?
let cfg = Config::default();
// Good: intent is explicit
let cfg = Config::read_only();
```

**Prefer the simplest solution that meets today's requirements.** Build the minimal correct shape and add complexity as real demands arrive. When you file follow-ups, make them actionable: break a vague concern into specific, concrete tasks rather than a catch-all note.
