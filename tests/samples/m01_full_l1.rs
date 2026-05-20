// Exercises every L1 syntax form.

fn add(x: i32, y: i32) -> i32 {
    x + y
}

fn main() {
    let mut counter = 0;
    let v = if counter == 0 {
        let t = add(2, 3);
        -t
    } else {
        0
    };
    let block_val = {
        1 + 2
    };
    let cmp = block_val <= v;
    let logical = !cmp && true;
    // expression statement: result discarded
    logical;
}
