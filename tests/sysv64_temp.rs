#![cfg(not(unix))]

use crate::common::{temp::tests_impl, tests_impl_recursive};

mod common;

tests_impl_recursive!(tests_impl, "sysv64");
