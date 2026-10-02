#[allow(unused)]
macro_rules! tests_impl {
    ($cc:literal, $tys:path, $argn:ident, $($args:ident,)*) => {
        pastey::paste! {
            unsafe extern $cc fn [<f_ $tys:lower $argn>]($($args: $tys,)*) -> $tys {
                <$tys>::default() $(+ $args)*
            }

            unsafe extern $cc fn [<f_mut_ $tys:lower $argn>]($($args: $tys,)*) -> $tys {
                <$tys>::default() $(+ $args)*
            }

            unsafe extern $cc fn [<f_once_ $tys:lower $argn>]($($args: $tys,)*) -> $tys {
                <$tys>::default() $(+ $args)*
            }

            #[test]
            fn [<static_hook_ $tys:lower $argn>]() {
                use diversion::hook::leak::StaticHook;

                let f = std::hint::black_box(
                    [<f_ $tys:lower $argn>] as unsafe extern $cc fn($($args: $tys,)*) -> $tys
                );

                let ($($args,)*) = Default::default();
                let call_f = {
                    let ($($args,)*) = ($(<$tys>::clone(&$args),)*);
                    move || unsafe {
                        f($(<$tys>::clone(&$args),)*)
                    }
                };

                let expected = std::sync::RwLock::new(call_f());

                crate::common::hook_contended(call_f.clone(), &expected, || unsafe {
                    for i in 1..5 {
                        let installer = diversion::install(f).unwrap();

                        let mut expected = expected.write().unwrap();

                        let hook = installer.static_hook(|hook| {
                            |$($args: $tys,)*| hook.call_original(($($args,)*)) + <$tys>::default()
                        });

                        let original = hook.call_original(($(<$tys>::clone(&$args),)*));
                        let hooked = call_f();

                        assert_eq!(original, *expected, "at depth {i}");

                        *expected = expected.clone() + <$tys>::default();
                        assert_eq!(hooked, *expected, "at depth {i}");
                    }
                });
            }

            #[test]
            fn [<static_hook_mut_ $tys:lower $argn>]() {
                use diversion::hook::leak::StaticHook;

                let f = std::hint::black_box(
                    [<f_mut_ $tys:lower $argn>] as unsafe extern $cc fn($($args: $tys,)*) -> $tys
                );

                let ($($args,)*) = Default::default();
                let call_f = {
                    let ($($args,)*) = ($(<$tys>::clone(&$args),)*);
                    move || unsafe {
                        f($(<$tys>::clone(&$args),)*)
                    }
                };

                let expected = std::sync::RwLock::new(call_f());

                crate::common::hook_contended(call_f.clone(), &expected, || unsafe {
                    for i in 1..5 {
                        let installer = diversion::install(f).unwrap();

                        let mut expected = expected.write().unwrap();

                        let hook = installer.static_hook_mut(|hook| {
                            |$($args: $tys,)*| hook.call_original(($($args,)*)) + <$tys>::default()
                        });

                        let original = hook.call_original(($(<$tys>::clone(&$args),)*));
                        let hooked = call_f();

                        assert_eq!(original, *expected, "at depth {i}");

                        *expected = expected.clone() + <$tys>::default();
                        assert_eq!(hooked, *expected, "at depth {i}");
                    }
                });
            }

            #[test]
            fn [<static_hook_once_ $tys:lower $argn>]() {
                let f = std::hint::black_box(
                    [<f_once_ $tys:lower $argn>] as unsafe extern $cc fn($($args: $tys,)*) -> $tys
                );

                let ($($args,)*) = Default::default();
                let call_f = || unsafe {
                    f($(<$tys>::clone(&$args),)*)
                };

                let expected_original = call_f();
                let expected_hooked = expected_original.clone() + <$tys>::default();

                unsafe {
                    let hook = diversion::static_hook_once(f, |hook| {
                        |$($args: $tys,)*| hook.call_original(($($args,)*)) + <$tys>::default()
                    })
                    .unwrap();

                    let original = hook.call_original(($(<$tys>::clone(&$args),)*));
                    let hooked = call_f();

                    assert_eq!(original, expected_original);
                    assert_eq!(hooked, expected_hooked);

                    let original = call_f();

                    assert_eq!(original, expected_original);
                }
            }
        }
    };
}

#[allow(unused)]
pub(crate) use tests_impl;
