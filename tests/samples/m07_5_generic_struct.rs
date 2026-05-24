struct Wrapper<T> {
    v: T,
}

fn main() {
    let w = Wrapper { v: 5 };
    let a = w.v;
}
