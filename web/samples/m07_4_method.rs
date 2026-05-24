struct Point { x: i32, y: i32 }

impl Point {
    fn x(&self) -> i32 {
        self.x
    }
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let v = p.x();
}
