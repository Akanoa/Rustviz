fn main() {
    let m1 = Arc::new(Mutex::new(0));
    let m2 = Arc::new(Mutex::new(0));
    let m1b = Arc::clone(&m1);
    let m2b = Arc::clone(&m2);
    let h = thread::spawn(move || {
        let a = m2b.lock();
        let b = m1b.lock();
    });
    let g1 = m1.lock();
    let g2 = m2.lock();
    h.join();
}
