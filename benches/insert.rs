use bloomset::BloomSet;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/*
static TEST_DATA: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7];
static OTHER_DATA: &[i32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
*/
static TEST_DATA: &[&str] = &["The", "Rust", "Programming", "Language"];
static OTHER_DATA: &[&str] = &["Is", "pretty", "amazing"];

pub fn bloomset_insert(c: &mut Criterion) {
    c.bench_function("BloomSet::insert", |b| {
        b.iter(|| {
            let mut set = BloomSet::with_capacity(TEST_DATA.len());
            for elem in TEST_DATA {
                set.insert(elem);
            }
            black_box(&set);
        })
    });
}

pub fn hashset_insert(c: &mut Criterion) {
    c.bench_function("HashSet::insert", |b| {
        b.iter(|| {
            let mut set = fnv::FnvHashSet::default();
            set.reserve(TEST_DATA.len());
            for elem in TEST_DATA {
                set.insert(elem);
            }
            black_box(&set);
        })
    });
}

criterion_group!(insert, bloomset_insert, hashset_insert);

pub fn bloomset_contains(c: &mut Criterion) {
    c.bench_function("BloomSet::contains", |b| {
        let mut set = BloomSet::with_capacity(TEST_DATA.len());
        for elem in TEST_DATA {
            set.insert(elem);
        }
        b.iter(|| {
            for elem in OTHER_DATA {
                black_box(set.contains(&elem));
            }
        })
    });
}

pub fn hashset_contains(c: &mut Criterion) {
    c.bench_function("HashSet::contains", |b| {
        let mut set = fnv::FnvHashSet::default();
        set.reserve(TEST_DATA.len());
        for elem in TEST_DATA {
            set.insert(elem);
        }

        b.iter(|| {
            for elem in OTHER_DATA {
                black_box(set.contains(&elem));
            }
        })
    });
}

criterion_group!(contains, bloomset_contains, hashset_contains);
criterion_main!(insert, contains);
