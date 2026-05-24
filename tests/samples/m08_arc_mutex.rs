fn main() {
    let m = Arc::new(Mutex::new(0));
    let m2 = Arc::clone(&m);
    let h = thread::spawn(move || {
        let g = m2.lock();
    });
    {
        let g = m.lock();
    };
    h.join();
}
