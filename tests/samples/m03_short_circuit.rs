fn main() {
    let a = true;
    let b = a || (1 / 0 == 0);
    let c = false;
    let d = c && (1 / 0 == 0);
}
