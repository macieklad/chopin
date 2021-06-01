mod expr;
mod parser;
mod scanner;

fn main() {
  let test_str = "class Foo {}\n\
                  var foo = Foo();\n\
                  foo.attr = 42;\n\
                  print foo.attr;";
  let tokens = scanner::scan_tokens(test_str.to_string()).unwrap();
  println!("{:?}", tokens);
}
