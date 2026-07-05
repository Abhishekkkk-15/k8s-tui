# Real Cluster Integration Plan

Goal: replace `MockBackend` with the real `Daemon` (kube-rs client), one resource
kind at a time, starting with Pods. Work through the checkboxes top to bottom —
each step is small and should compile before you move to the next.

## Why this architecture (read once, refer back if confused)

- The render loop (`main.rs::run`) is **synchronous** — it calls `terminal.draw`
  and polls for keys in a plain `while` loop, ticking every 120ms.
- The kube-rs client is **async** — `Api::list()` etc. return futures.
- Calling an async k8s API directly from the sync render loop would block
  the whole UI (including keypresses) on every network round-trip.
- Fix: a **background tokio task** owns the fetching, writes results into a
  shared cache (`Arc<Mutex<Vec<PodInfo>>>`), and the UI thread only ever does
  a fast, synchronous read of the cache. The UI never waits on the network.
- `ResourceRow` (in `src/data/mock.rs`) is the backend-agnostic shape the UI
  renders (see `src/ui/table.rs`). Both mock and real data must end up as
  `ResourceRow`s — that's the seam where `App::visible_rows()` picks a source.

---

## Milestone 1 — Real Pods, end to end

- [ ] **Step 1: Add a pod cache to `Daemon`**
  File: `src/daemon/daemon.rs`
  - Add `pods_cache: Arc<Mutex<Vec<PodInfo>>>` field to `Daemon`.
  - `#[derive(Clone)]` on `Daemon` (both `Client` and `Arc` are cheap to clone).
  - Initialize `pods_cache` to `Arc::new(Mutex::new(Vec::new()))` in `Daemon::new()`.
  - Import `crate::data::PodInfo`.
  - *Why*: shared, thread-safe storage the background task writes to and the
    UI reads from without blocking on either side.

- [ ] **Step 2: Write an async fetch-and-convert function**
  File: `src/daemon/daemon.rs`
  - Add `async fn fetch_pods(&self) -> Result<Vec<PodInfo>, ...>` that:
    - Lists pods across all namespaces (`Api::all(self.client.clone())`, not
      `default_namespaced` — the UI needs every namespace).
    - Converts each `k8s_openapi::api::core::v1::Pod` into a `PodInfo`
      (name, namespace, ready count, phase, restarts, node, age, etc. — see
      `PodInfo` fields in `src/data/model.rs` and how `MockBackend` fakes them
      in `src/data/mock.rs` for what each field means).
  - *Why*: this is the one place raw k8s API types get translated into the
    app's own model — keep it isolated here so nothing else needs to know
    about `k8s_openapi` types.

- [ ] **Step 3: Spawn a background polling task**
  File: `src/main.rs`
  - After `Daemon::new()` succeeds and before `run(...)`, `tokio::spawn` a
    loop: call `fetch_pods()`, lock `pods_cache` and replace its contents,
    then `tokio::time::sleep` (start with ~2s, matching `DATA_TICK` in
    `app.rs`) and repeat.
  - Move a **clone** of `Daemon` into the spawned task; keep the original
    to pass into `App::new()`.
  - *Why*: this is what keeps the cache warm without the UI thread ever
    calling into kube-rs directly.

- [ ] **Step 4: Add a synchronous cache getter**
  File: `src/daemon/daemon.rs`
  - Add `fn pods(&self) -> Vec<PodInfo>` (no `async`) that locks
    `pods_cache` and returns a clone of the `Vec`.
  - *Why*: this is the only method the UI thread is allowed to call — it's
    fast (just a mutex lock + clone) and never touches the network.

- [ ] **Step 5: Extract a shared `PodInfo -> ResourceRow` mapping**
  File: `src/data/mock.rs` (move logic out) — new home can be a free fn in
  `src/data/mod.rs` or alongside `ResourceRow`, e.g. `pub fn pod_row(p: &PodInfo) -> ResourceRow`.
  - `MockBackend::rows()` already builds `ResourceRow`s for pods inline
    (around `src/data/mock.rs:263`) — pull that mapping out into a
    standalone function and call it from both `MockBackend` and the new
    real-data path.
  - *Why*: avoids two copies of the same cell-formatting logic drifting
    apart later.

- [ ] **Step 6: Wire real pods into `App::visible_rows`**
  File: `src/app.rs` (~line 127)
  - In `visible_rows()`, branch: if `kind == ResourceKind::Pods`, build rows
    from `self.daemon.pods()` (mapped via the Step 5 function) instead of
    `self.backend.rows(kind, ns)`. Keep applying the existing namespace +
    text filter logic on top, same as today.
  - Everything else (Deployments, Services, ...) still goes through
    `self.backend` for now.
  - *Why*: this is the actual cutover point — once this compiles and runs,
    the Pods table shows your real cluster.

**Milestone 1 done when:** running the app, pressing into the Pods table
shows pods from your actual kube context (try `minikube` or `kind`), and they
update roughly every couple seconds without input lag.

---

## Milestone 2 — Generalize the pattern to other resource kinds

- [ ] Repeat Steps 1-2 pattern for `Deployments` (own cache field or a
  small `HashMap<ResourceKind, Vec<ResourceRow>>` cache — decide once you've
  done Pods and see if per-kind fields feel repetitive).
- [ ] Same for `Services`, `Namespaces`, `Nodes`, `ConfigMaps`, `Secrets`,
  `Pvcs`, `Events`, `ReplicaSets`, `StatefulSets`, `Ingresses` — whichever
  order is useful to you; Namespaces/Nodes are good next picks since the
  cluster picker view already depends on real namespace names for
  `cycle_namespace()` in `app.rs`.
- [ ] Once every kind has a real source, decide whether `MockBackend` is
  still needed as a fallback/demo mode, or can be deleted.

## Milestone 3 — Wire mutations to the real cluster

- [ ] `start_create` / `on_key_create` (`app.rs`) currently calls
  `self.backend.create_default(...)` — replace with real `Api::create`
  calls per kind (only for kinds where `ResourceKind::creatable()` is true).
- [ ] `start_delete_confirm` / `on_key_confirm_delete` — replace
  `self.backend.delete(...)` with real `Api::delete`.
- [ ] `start_edit` / `on_key_edit` — replace `self.backend.apply_edit(...)`
  and `current_edit_value(...)` with real `Api::patch`/read of the live
  object (only for kinds where `ResourceKind::editable()` is true).
- [ ] These are all async kube calls triggered by a keypress, not a poll
  loop — decide whether to block briefly (single mutation, user is already
  waiting) or spawn + surface a "pending" status message and reconcile via
  the next cache refresh.

## Milestone 4 — Cleanup

- [ ] Remove `MockBackend` and mock-only fields on `App` once nothing calls
  into it.
- [ ] Reconsider polling vs. `kube::runtime::watcher` (push-based updates
  instead of a fixed-interval list loop) if polling feels laggy or wasteful.
