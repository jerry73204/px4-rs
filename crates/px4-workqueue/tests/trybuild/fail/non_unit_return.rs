#![feature(type_alias_impl_trait)]

use px4_workqueue::task;

#[task(wq = "test1")]
async fn returns_int() -> u32 {
    7
}

fn main() {}
