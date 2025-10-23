# Harmoniq Real-time DSP Engine

## Overview

The new DSP subsystem is implemented in two layers:

1. **`harmoniq-dsp` crate** – reusable zero-allocation DSP primitives and buffer
   abstractions (`AudioBlock`, filters, gain, saturator, stereo delay, pan law,
   and smoothing utilities). These components are designed to be shared across
   the engine, plugins, and future render paths without pulling in the rest of
   the engine state.
2. **`harmoniq-engine::dsp` module** – a real-time safe graph executor that owns
   DSP nodes, parameter/event fan-out, and the OpenASIO callback integration via
   `RealtimeDspEngine`.

The DSP graph executes as a pre-planned serial pipeline (topologically sorted)
with per-node scratch buffers allocated during `prepare`. No heap allocations,
locks, or syscalls occur on the audio thread.

## Buffer Model

`AudioBlock`/`AudioBlockMut` are non-owning views over either interleaved or
planar sample layouts. They expose strided sample access so nodes can operate on
SIMD-friendly contiguous memory when available, while still supporting backend
callbacks that deliver planar buffers. All constructors are `unsafe` and expect
callers to provide valid pointers and frame counts, allowing the views to be
created without additional allocations.

## Graph Execution

`DspGraph` manages node registration, topology planning, and per-node parameter
queues. Nodes implement the `DspNode` trait, receiving a `ProcessContext` that
provides audio blocks, transport information, and MIDI/event slices. During
`process` the graph drains each node’s lock-free `ringbuf` consumer before
calling `DspNode::process`, ensuring parameter updates are handled outside of
inner sample loops.

Scratch buffers are resized during `prepare` using the configured maximum block
size and output channel count. The last node always writes directly into the
callback’s output buffer; intermediate nodes write into scratch buffers that are
reused across blocks.

## Parameter & Event Delivery

Every node can request a dedicated single-producer/single-consumer ring buffer
for parameter updates. Producers are wrapped in `ParamPort` handles that can be
shared with control threads. MIDI/events are broadcast to all nodes each block
via a separate ring buffer that feeds an `ArrayVec` backed staging buffer – no
allocation occurs on the audio thread, and excess events are safely dropped when
capacity is exceeded.

Transport state is stored in a lock-free `TransportClock` using atomics so UI or
sequencer threads can update tempo, time signature, and timeline position
without synchronisation primitives.

## Real-time Integration

`RealtimeDspEngine` implements `EngineRt` so it can be plugged directly into the
OpenASIO backend. It:

- Prepares the graph and flushes denormals on start-up.
- Converts `AudioView`/`AudioViewMut` from the backend into `AudioBlock`
  views without additional allocations.
- Drains the MIDI ring buffer into a fixed-size `ArrayVec` before invoking the
  graph.
- Optionally enables per-block FTZ/DAZ guards when the `no-denormals` feature is
  active.

The engine exposes handles for parameter ports, MIDI submission, and transport
updates so higher-level components can drive the graph without touching audio
thread internals.
