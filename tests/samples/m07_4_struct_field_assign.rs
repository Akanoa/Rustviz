struct Point { x: i32, y: i32 }

fn main() {
    let mut p = Point { x: 1, y: 2 };
    p.x = 5;
    let a = p.x;
}
