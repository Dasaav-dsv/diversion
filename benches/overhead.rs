use std::arch::naked_asm;

use criterion::{Criterion, criterion_group, criterion_main};
use diversion::{hook::temp::TemporaryHook, install};

criterion_main!(benches);
criterion_group!(benches, criterion_benchmark);

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("add", |b| {
        b.iter(|| unsafe {
            add(123, -123);
        })
    });

    let _hook = unsafe {
        install(add as unsafe extern "win64" fn(_, _) -> _)
            .unwrap()
            .hook(|_| |a, b| a + b)
    };

    c.bench_function("add_hooked", |b| {
        b.iter(|| unsafe {
            add(123, -123);
        })
    });
}

#[unsafe(naked)]
unsafe extern "win64" fn add(a: i32, b: i32) -> i32 {
    naked_asm!("lea eax,[rcx+rdx]", "ret", "int3")
}
