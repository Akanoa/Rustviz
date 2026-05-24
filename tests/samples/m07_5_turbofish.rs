fn id<T>(x: T) -> T {
    x
}

fn main() {
    let v = id::<bool>(false);
}
