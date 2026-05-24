struct Point { x: i32, y: i32 }

trait Show {
    fn show(&self) -> i32;
}

trait Counter {
    fn count(&self) -> i32;
}

impl Show for Point {
    fn show(&self) -> i32 {
        self.x
    }
}

impl Counter for Point {
    fn count(&self) -> i32 {
        self.y
    }
}

fn show_n_count<T: Show + Counter>(x: T) -> i32 {
    x.show() + x.count()
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let r = show_n_count(p);
}
