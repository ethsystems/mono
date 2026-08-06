//! Engine apply-path overhead: pure bookkeeping cost isolated behind a no-op fold.

use std::hint::black_box;

use chainfold::{
    Batch,
    BlockRef,
    BlockSpan,
    Engine,
    EngineConfig,
    LogEvent,
    test_util::NoopFold,
};
use criterion::{
    BatchSize,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};

/// Observed-block ring window, sized past the warmup block plus every timed span.
const RING_CAPACITY: usize = 128;
/// Blocks the timed batch spans.
const SPAN_COUNT: u64 = 64;
/// Events carried by each timed span.
const EVENTS_PER_SPAN: u64 = 64;
/// Total events the timed batch carries.
const EVENT_COUNT: u64 = SPAN_COUNT * EVENTS_PER_SPAN;

/// Builds a distinguishable header for a block number.
fn block_ref(number: u64) -> BlockRef {
    let mut hash = [0u8; 32];
    hash[..8].copy_from_slice(&number.to_le_bytes());
    BlockRef { number, hash }
}

/// Builds a fresh engine and applies one warmup block, so the timed batch carries a
/// boundary that matches the ring's newest entry.
fn warmed_engine() -> Engine<NoopFold> {
    let config = EngineConfig {
        ring_capacity: RING_CAPACITY,
        checkpoint_slots: 0,
    };
    let mut engine = Engine::new(NoopFold, config).expect("engine config is valid");
    let warmup = Batch {
        boundary: None,
        spans: vec![BlockSpan {
            block: block_ref(0),
            start: 0,
            end: 1,
        }],
        events: vec![LogEvent {
            log_index: 0,
            event: 0,
        }],
    };
    engine
        .apply_batch(&warmup)
        .expect("warmup batch applies cleanly");
    engine
}

/// Builds the batch under measurement: 4096 events over 64 spans past the warmup block.
fn timed_batch() -> Batch<u64> {
    let mut events = Vec::with_capacity(EVENT_COUNT as usize);
    let mut spans = Vec::with_capacity(SPAN_COUNT as usize);
    for block in 1..=SPAN_COUNT {
        let start = events.len() as u32;
        for log_index in 0..EVENTS_PER_SPAN {
            events.push(LogEvent {
                log_index,
                event: log_index,
            });
        }
        let end = events.len() as u32;
        spans.push(BlockSpan {
            block: block_ref(block),
            start,
            end,
        });
    }
    Batch {
        boundary: Some(block_ref(0)),
        spans,
        events,
    }
}

fn bench_apply(c: &mut Criterion) {
    let batch = timed_batch();
    let mut group = c.benchmark_group("chainfold::apply");
    group.throughput(Throughput::Elements(EVENT_COUNT));
    group.bench_function(format!("fold=noop n={EVENT_COUNT}"), |b| {
        b.iter_batched(
            warmed_engine,
            |mut engine| {
                engine
                    .apply_batch(black_box(&batch))
                    .expect("apply_batch succeeds");
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(benches, bench_apply);
criterion_main!(benches);
