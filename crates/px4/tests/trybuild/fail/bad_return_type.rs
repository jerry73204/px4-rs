use px4::main;

// `&'static str` does not implement `ModuleResult`. The macro
// emits `ModuleResult::into_c_int(result, ...)`, so the trait
// bound fails at the call site rather than inside the macro.
#[main]
fn entry() -> &'static str {
    "nope"
}

fn main() {}
