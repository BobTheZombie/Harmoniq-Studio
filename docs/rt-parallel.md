# Real-time Parallel Scheduling

Harmoniq Studio's real-time engine now exposes a parallel configuration that
controls how many helper threads are spawned alongside the hard real-time audio
callback. The design keeps the audio callback free from allocations and heavy
synchronisation; it only coordinates preallocated jobs for workers that operate
on the processing graph.

## Depth-parallel execution

The scheduler organises graph nodes into topological spans. Nodes that are
marked `parallel_safe` can be scheduled onto the worker pool while the callback
continues processing sequential nodes. This preserves determinism because nodes
that are not marked safe are processed in order on the audio thread.

## CPU affinity

A helper in `rt::cpu` computes reasonable defaults for pinning the callback and
workers to CPU cores. When the `core_affinity` feature is enabled on Linux the
threads attempt to bind to those cores; on other platforms the helper becomes a
no-op so the engine remains portable.

## Determinism and safety

The callback never performs heap allocations or blocking operations. Tests in
`crates/harmoniq-engine/tests` assert that the guarded allocator records no
allocations while rendering and that parallel execution produces the same output
as the single-threaded baseline.

To enable worker threads from the UI, update the engine's `RtParallelCfg` and
call `Engine::rebuild()`—the pool grows or shrinks without disturbing the rest
of the engine state.
