//! Hot paths of the engine, with criterion: loading the compiled-in catalog and resolving plans.
//! Run with `cargo bench -p bindle-core`; `bench/run.sh` runs these and the command timings.
use bindle_core::plan::resolve;
use bindle_core::{Catalog, Facts, Selection};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn arch_facts() -> Facts {
    Facts { distro_id: "arch".into(), virt: "none".into(), ..Default::default() }
}

fn catalog_load(c: &mut Criterion) {
    c.bench_function("catalog_load", |b| b.iter(|| black_box(Catalog::arch().unwrap())));
}

fn plans(c: &mut Criterion) {
    let catalog = Catalog::arch().unwrap();
    let facts = arch_facts();
    for profile in catalog.profiles.iter().map(|p| p.id.clone()) {
        let sel = Selection { profile: Some(profile.clone()), ..Default::default() };
        c.bench_function(&format!("plan_profile_{profile}"), |b| {
            b.iter(|| black_box(resolve(&catalog, &facts, &sel, &|_| None).unwrap()))
        });
    }
}

criterion_group!(benches, catalog_load, plans);
criterion_main!(benches);
