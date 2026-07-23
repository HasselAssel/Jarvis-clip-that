mod recorders;
mod macros;

fn main() {
    let a = debug!(1 + 2 + 3);
    println!("{}", a);
}
