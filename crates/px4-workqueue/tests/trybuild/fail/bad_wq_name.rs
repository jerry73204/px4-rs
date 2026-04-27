#![feature(type_alias_impl_trait)]

use px4_workqueue::task;

#[task(wq = "no_such_queue")]
async fn nope() {}

fn main() {}
