fn main() {
    let mut v: Vec<i32> = Vec::new();
    v.push(1);
    v.push(2);
    let r = &v[0];
    let b = Box::new(99);
    v.push(3);
}
