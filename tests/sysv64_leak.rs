#![cfg(not(unix))]

use crate::common::{leak::tests_impl, tests_impl_recursive};

mod common;

tests_impl_recursive!(tests_impl, "sysv64");
