fn main() {
    let counter = Arc::new(Mutex::new(0));
    let inc = Arc::clone(&counter);
    let dec = Arc::clone(&counter);
    let h1 = thread::spawn(move || {
        let g = inc.lock();
    });
    let h2 = thread::spawn(move || {
        let g = dec.lock();
    });
    h1.join();
    h2.join();
}
