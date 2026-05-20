fn foo() {}
fn bar() {}

fn main() {
    let c = true;
    let v = if c { 1 } else { 2 };
    if c {
        foo();
    } else {
        bar();
    };
}
