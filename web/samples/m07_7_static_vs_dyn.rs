struct Point { x: i32, y: i32 }

trait Show {
    fn show(&self) -> i32;
}

impl Show for Point {
    fn show(&self) -> i32 {
        self.x
    }
}

fn s<T: Show>(x: T) -> i32 {
    x.show()
}

fn d(x: &dyn Show) -> i32 {
    x.show()
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let a = s(p);
    let b = d(&p);
}
