struct Point { x: i32, y: i32 }

trait Counter {
    fn count(&self) -> i32;
    fn double(&self) -> i32 {
        self.count() * 2
    }
}

impl Counter for Point {
    fn count(&self) -> i32 {
        self.x
    }
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let v = p.double();
}
