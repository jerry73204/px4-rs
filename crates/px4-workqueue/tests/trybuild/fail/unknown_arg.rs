#![feature(type_alias_impl_trait)]

use px4_workqueue::task;

#[task(wq = "test1", priority = "high")]
async fn nope() {}

fn main() {}
