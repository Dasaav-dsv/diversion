use std::arch::naked_asm;

use criterion::{Criterion, criterion_group, criterion_main};
use diversion::{
    hook::{custom::install_custom, temp::TemporaryHook},
    install,
};

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

    c.bench_function("ret", |b| b.iter(|| unsafe { ret() }));

    let _hook = unsafe { install_custom(ret as *const ()).unwrap().hook(|_| || ()) };

    c.bench_function("ret_hooked", |b| b.iter(|| unsafe { ret() }));
}

#[unsafe(naked)]
unsafe extern "win64" fn add(a: i32, b: i32) -> i32 {
    naked_asm!("lea eax,[rcx+rdx]", "ret", "int3")
}

#[unsafe(naked)]
unsafe extern "win64" fn ret() {
    naked_asm!("ret 0", "int3", "int3")
}
