struct Point { x: i32, y: i32 }

trait Show {
    fn show(&self) -> i32;
}

impl Show for Point {
    fn show(&self) -> i32 {
        self.x
    }
}

fn print(x: &dyn Show) -> i32 {
    x.show()
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let r = print(&p);
}
