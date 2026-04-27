#![feature(type_alias_impl_trait)]

use px4_workqueue::task;

#[task]
async fn nope() {}

fn main() {}
