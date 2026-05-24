fn main() {
    let a = Arc::new(5);
    {
        let b = Arc::clone(&a);
    }
}
