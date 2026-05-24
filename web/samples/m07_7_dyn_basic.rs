struct Point { x: i32, y: i32 }

trait Show {
    fn show(&self) -> i32;
}

impl Show for Point {
    fn show(&self) -> i32 {
        self.x
    }
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let d: &dyn Show = &p;
    let s = d.show();
}
