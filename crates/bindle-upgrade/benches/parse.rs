//! bindleUP's parsers on a big, synthetic `checkupdates` listing and Arch news feed. These run on
//! every `bindleup` start, so a slowdown here is felt before the first screen.
use bindle_upgrade::{news, parse};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn parsers(c: &mut Criterion) {
    // A heavy upgrade day: 1500 packages.
    let listing: String = (0..1500).map(|i| format!("pkg-{i} 1.{i}.0-1 -> 1.{i}.1-1\n")).collect();
    c.bench_function("parse_updates_1500", |b| b.iter(|| black_box(parse::updates(&listing))));

    let feed: String = (0..200)
        .map(|i| {
            format!(
                "<item><title>News item {i} &amp; more</title><link>https://archlinux.org/news/{i}/</link>\
                 <pubDate>Mon, 0{} Jan 2026 10:00:00 +0000</pubDate></item>\n",
                1 + i % 9
            )
        })
        .collect();
    c.bench_function("parse_news_200", |b| b.iter(|| black_box(news::parse(&feed))));
}

criterion_group!(benches, parsers);
criterion_main!(benches);
