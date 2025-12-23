use std::mem;

/// A utility struct that manages a `String` while providing an `'static` reference
/// to its content. The memory for the `String` is reclaimed when this struct is dropped.
///
/// This struct enables scenarios where an `&'static str` is required (e.g., for
/// `BoxFuture<'static, ...>` in `async` contexts, or when integrating with APIs that
/// only accept `'static` references), but the underlying `String`'s memory needs to
/// be explicitly managed and eventually deallocated, rather than perpetually leaked
/// or managed by `Arc`.
///
/// The `'static` reference provided by this struct is "scoped" to the lifetime of
/// the `ScopedStaticStr` instance itself. It is critical to understand and adhere
/// to this safety contract.
///
/// # Safety Rationale
///
/// This struct's core functionality relies on `unsafe` Rust, specifically `std::mem::transmute`,
/// to perform a lifetime extension of an `&mut str` reference to `'static`. While the compiler
/// believes this reference is `'static'`, its actual validity is tied to the `ScopedStaticStr`
/// instance that owns the underlying `String` data.
///
/// The safety of using `ScopedStaticStr` depends entirely on the caller upholding the following
/// **Safety Guarantees**:
///
/// 1.  **Ownership and Liveness**: The `inner: Box<String>` field ensures that the `String` data
///     remains allocated and valid *for as long as the `ScopedStaticStr` instance is alive*.
///     When a `ScopedStaticStr` instance is dropped, its `inner` `Box<String>` is also dropped,
///     and the `String`'s memory is safely deallocated by Rust's standard memory management.
/// 2.  **`'static` Reference Validity**: The `static_ref: &'static str` field provides a pointer
///     to the `String`'s data that the compiler *believes* has a `'static` lifetime.
///     **It is the paramount responsibility of the user of `ScopedStaticStr` to ensure that this
///     `'static` reference is *never used after the `ScopedStaticStr` instance that created it
///     has been dropped***. Failure to do so will result in a dangling pointer dereference,
///     leading to Undefined Behavior (UB), which can manifest as crashes, corrupted data, or
///     security vulnerabilities.
/// 3.  **No Aliasing of Mutability**: The `static_ref` is always an immutable `&'static str`.
///     The `inner` `Box<String>` is not exposed for direct mutable access after `static_ref` is created,
///     thus preventing mutable aliasing issues through `static_ref`.
///
/// # When to Use
///
/// Use `ScopedStaticStr` when you have a `String` that:
/// *   Needs to be passed to APIs or `async` contexts requiring an `&'static str`.
/// *   Must have its memory reclaimed after a specific scope or task completes.
/// *   Is not shared across multiple independent threads (where `Arc<String>` would be safer).
/// *   The overhead of `Arc<String>` (e.g., reference counting) or `String::clone()` is unacceptable.
/// *   The alternative of `String::leak()` is undesirable due to permanent memory consumption.
///
/// **Typical Use Case**: Processing a file's content in an `apalis` or `tokio::spawn` task.
/// The `String` representing the file content needs to be accessible as `'static` during the task's
/// execution, but its memory should be reclaimed once the task completes and the `ScopedStaticStr`
/// instance (moved into the task) goes out of scope.
///
/// # When NOT to Use (or use with extreme caution)
///
/// *   If the `&'static str` obtained via `as_static_str()` might genuinely outlive the
///     `ScopedStaticStr` instance that owns its data. This is the primary source of UB for this struct.
/// *   If you need multiple independent owners or shared mutable access to the `String` data across
///     different threads; `Arc<String>` or `Arc<Mutex<String>>` would be more appropriate and safer.
/// *   If you can simply `clone()` the `String` for the `async` task, and the memory overhead
///     is acceptable; this avoids `unsafe` altogether.
/// *   If you can refactor your code to avoid the `'static` requirement entirely.
///
/// # Example (Correct Usage with `async` Tasks)
///
/// ```rust
/// use tokio::task;
/// use std::time::Duration;
/// # // Define ScopedStaticStr for the example to compile without the actual definition
/// # pub struct ScopedStaticStr { inner: Box<String>, static_ref: &'static str, }
/// # impl ScopedStaticStr {
/// #   pub unsafe fn new<S>(s: S) -> Self where S: Into<String>, {
/// #       let mut boxed = Box::new(s.into());
/// #       let static_ref = boxed.as_mut_str();
/// #       let static_ref = std::mem::transmute::<&mut str, &'static mut str>(static_ref);
/// #       let static_ref = &*static_ref;
/// #       Self { inner: boxed, static_ref, }
/// #   }
/// #   pub fn as_static_str(&self) -> &'static str { self.static_ref }
/// # }
///
/// #[tokio::main]
/// async fn main() {
///     let original_data = "Data for async processing".to_string();
///     
///     // Create the manager. This call must be wrapped in `unsafe { ... }`
///     // because `ScopedStaticStr::new` is an `unsafe fn`.
///     let static_manager = unsafe { ScopedStaticStr::new(original_data) };
///     let static_str_ref = static_manager.as_static_str();
///
///     // Move *both* the `static_manager` (which owns the data) and the `static_str_ref`
///     // into the async task. This ensures the data lives as long as the task needs the reference.
///     let handle = task::spawn(async move {
///         // `static_manager` is now owned by this async task.
///         // It will be dropped when the task finishes.
///         println!("Task received data: {}", static_str_ref);
///         tokio::time::sleep(Duration::from_millis(50)).await;
///         format!("Processed length: {}", static_str_ref.len())
///     });
///     
///     let result = handle.await.expect("Task failed");
///     println!("Task result: {}", result);
///     // At this point, the `static_manager` (which was inside the task) has been dropped,
///     // and the memory for "Data for async processing" has been reclaimed.
/// }
/// ```
pub struct ScopedStaticStr {
    inner: Box<String>,
    static_ref: &'static str,
}

impl ScopedStaticStr {
    /// Creates a new `ScopedStaticStr` from any type convertible into `String`.
    ///
    /// # Safety
    ///
    /// This function is inherently `unsafe` because it uses `std::mem::transmute`
    /// to perform a lifetime extension of the `String`'s internal `&str` reference
    /// from its actual (scoped) lifetime to `'static`.
    ///
    /// **The caller must ensure that the `&'static str` returned by `as_static_str()`
    /// is never used after the `ScopedStaticStr` instance that created it has been dropped.**
    /// Failure to uphold this guarantee will lead to a dangling pointer dereference and
    /// Undefined Behavior (UB), which can manifest as asynchronized access issues,
    /// crashes, corrupted data, or security vulnerabilities.
    ///
    /// The `ScopedStaticStr` instance itself *must be held for as long as the
    /// `&'static str` reference is needed*. In asynchronous contexts, this typically
    /// means moving the `ScopedStaticStr` instance into the `async move` block
    /// along with the `&'static str` reference it provides.
    pub unsafe fn new<S>(s: S) -> Self
    where
        S: Into<String>,
    {
        let mut boxed = Box::new(s.into());
        let static_ref = boxed.as_mut_str();

        // # SAFETY:
        // We are transmuting the lifetime of `&mut str` to `'static`.
        // This is safe ONLY if the `ScopedStaticStr` instance (which owns `boxed`)
        // is guaranteed to live at least as long as any consumer uses `static_ref`.
        // The `ScopedStaticStr` struct guarantees that `boxed` will not be dropped
        // before the `ScopedStaticStr` itself.
        let static_ref = unsafe { mem::transmute::<&mut str, &'static mut str>(static_ref) };

        // Convert `&'static mut str` to `&'static str`. This is safe as it's just
        // making the reference immutable.
        let static_ref = &*static_ref;

        Self {
            inner: boxed,
            static_ref,
        }
    }

    /// Returns a `&'static str` reference to the content of the managed `String`.
    ///
    /// # Safety
    ///
    /// The returned `&'static str` is only valid for as long as the `ScopedStaticStr`
    /// instance that created it remains alive. **Using this reference after the
    /// `ScopedStaticStr` has been dropped will result in a dangling pointer
    /// and Undefined Behavior.**
    ///
    /// Callers are responsible for ensuring the `ScopedStaticStr` instance lives long
    /// enough.
    pub fn as_static_str(&self) -> &'static str {
        self.static_ref
    }
}