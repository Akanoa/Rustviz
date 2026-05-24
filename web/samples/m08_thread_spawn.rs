fn main() {
    let h = thread::spawn(move || {
        let x = 5;
        let y = x + 1;
    });
    h.join();
}
