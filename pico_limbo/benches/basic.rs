use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use pico_limbo_lib::bench_support::{
    BenchProtocol, HotChunkCache, PROTOCOLS, VIEW_DISTANCES, chunk_cache_cold,
    collect_chunk_packets, component_json, component_legacy, component_nbt,
    decode_representative_packets, default_config_parse, drain_mixed_batch, drain_raw_cache_batch,
    encode_chunk_packets, encode_representative_packets, escape_minimessage, format_lobby_chat,
    lobby_heavy_config_parse, nbt_from_slice, nbt_to_bytes, private_message_packets,
};
use std::hint::black_box;
use tokio::runtime::Runtime;

fn bench_chunk_prep(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_prep");
    for protocol in PROTOCOLS {
        for view_distance in VIEW_DISTANCES {
            group.bench_with_input(
                BenchmarkId::new(protocol.label(), view_distance),
                &(protocol, view_distance),
                |b, &(protocol, view_distance)| {
                    b.iter(|| {
                        black_box(collect_chunk_packets(
                            black_box(protocol),
                            black_box(view_distance),
                        ))
                    });
                },
            );
        }
    }

    for protocol in [
        BenchProtocol::Legacy,
        BenchProtocol::LightUpdate,
        BenchProtocol::Latest,
    ] {
        group.bench_function(format!("encode/{}", protocol.label()), |b| {
            b.iter(|| black_box(encode_chunk_packets(black_box(protocol), black_box(2))));
        });
    }
    group.finish();
}

fn bench_chunk_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_cache");
    for protocol in PROTOCOLS {
        group.bench_function(format!("cold/{}", protocol.label()), |b| {
            b.iter(|| black_box(chunk_cache_cold(black_box(protocol), black_box(2))));
        });
        let hot_cache = HotChunkCache::new(protocol, 2);
        group.bench_function(format!("hot/{}", protocol.label()), |b| {
            b.iter(|| black_box(hot_cache.get()));
        });
    }
    group.finish();
}

fn bench_batch(c: &mut Criterion) {
    let rt = Runtime::new().expect("create tokio runtime");
    let mut group = c.benchmark_group("batch");

    group.bench_function("mixed_registry_stream", |b| {
        b.to_async(&rt)
            .iter(|| async { black_box(drain_mixed_batch().await) });
    });
    group.bench_function("raw_cache_stream", |b| {
        b.to_async(&rt)
            .iter(|| async { black_box(drain_raw_cache_batch().await) });
    });

    group.finish();
}

fn bench_packet_codec(c: &mut Criterion) {
    let mut group = c.benchmark_group("packet_codec");
    for protocol in PROTOCOLS {
        group.bench_function(format!("encode/{}", protocol.label()), |b| {
            b.iter(|| black_box(encode_representative_packets(black_box(protocol))));
        });
    }
    group.bench_function("decode/inbound_fixtures", |b| {
        b.iter(|| black_box(decode_representative_packets()));
    });
    group.finish();
}

fn bench_chat_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat_text");
    group.bench_function("lobby_chat_format", |b| {
        b.iter(|| black_box(format_lobby_chat()));
    });
    group.bench_function("private_message_packets", |b| {
        b.iter(|| black_box(private_message_packets()));
    });
    group.bench_function("minimessage_escape", |b| {
        b.iter(|| black_box(escape_minimessage()));
    });
    group.bench_function("component_json", |b| {
        b.iter(|| black_box(component_json()));
    });
    group.bench_function("component_nbt", |b| {
        b.iter(|| black_box(component_nbt()));
    });
    group.bench_function("component_legacy", |b| {
        b.iter(|| black_box(component_legacy()));
    });
    group.finish();
}

fn bench_nbt_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("nbt_config");
    group.bench_function("nbt_to_bytes", |b| {
        b.iter(|| black_box(nbt_to_bytes()));
    });
    group.bench_function("nbt_from_slice", |b| {
        b.iter(|| black_box(nbt_from_slice()));
    });
    group.bench_function("default_config_parse", |b| {
        b.iter(|| black_box(default_config_parse()));
    });
    group.bench_function("lobby_heavy_config_parse", |b| {
        b.iter(|| black_box(lobby_heavy_config_parse()));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_chunk_prep,
    bench_chunk_cache,
    bench_batch,
    bench_packet_codec,
    bench_chat_text,
    bench_nbt_config
);
criterion_main!(benches);
