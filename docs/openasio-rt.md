# OpenASIO real-time backend

The OpenASIO backend in `harmoniq_engine::rt::backend` is designed for a strict
real-time (RT) environment.

## Callback invariants

* `harmoniq_asio_audio_cb` performs **no** heap allocation, locking, logging, or
  system calls. All state is preallocated during `AudioBackend::open`.
* The callback forwards the first planar channel pointer for both input and
  output to the engine trampoline and increments lock-free sequence counters for
  watchdog metrics.
* Any deviation from the negotiated block size increments an RT xrun counter.

## Service-thread lifecycle

1. `open` — resolve the driver path, open the device, negotiate the stream
   configuration, and build the `RtTrampoline`. This step may re-open the driver
   but **must not** run while streaming.
2. `start` — schedule audio I/O with the negotiated configuration.
3. `stop` — halt the driver and wait at least one period before reconfiguring.
4. `close` — drop the driver handle and trampoline once streaming is stopped.

Always follow the sequence `stop → close → open → start` when changing devices or
buffer sizes to avoid moving the trampoline while the RT thread is running.
