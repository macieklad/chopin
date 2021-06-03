extern crate clap;
extern crate pest;
extern crate pest_string;

#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate pest_string_derive;

use clap::{App, Arg};
use pest_string::parser::Error;
use pest_string::StringParser;
use std::fs;

mod interpreter;

#[derive(Parser, StringParser)]
#[grammar = "grammar.pest"]
struct ChopinParser;

fn main() {
    let matches = App::new("Interpreter chopin")
        .version("1.0")
        .author("Maciej Ładoś, Michał Bar, Kamil Woźniak")
        .about("Prosty interpreter własnego języka programowania")
        .arg(
            Arg::with_name("source_file")
                .help("Plik z kodem źródłowym języka")
                .required(true)
                .index(1),
        )
        .get_matches();

    let input_file = matches.value_of("source_file").unwrap();
    let contents = fs::read_to_string(input_file).expect("File couldn't not be read");
    let statements = ChopinParser::parse_string(contents);

    match statements {
        Ok(_) => {}
        Err(err) => {
            println!("Parsing Error: {:?}", err);
            std::process::exit(-1);
        }
    }

    let mut interpreter: interpreter::Interpreter = Default::default();
    let results = interpreter.interpret(&statements.unwrap());
    match results {
        Ok(_) => {
            std::process::exit(0);
        }
        Err(err) => {
            println!(
                "Runtime Error: {}\n\n{}",
                err,
                interpreter.format_backtrace()
            );
            std::process::exit(-1);
        }
    }
}
