extern crate r6;

use std::collections::HashMap;
use std::rc::Rc;
use std::io::BufReader;
use std::borrow::Cow;
use r6::runtime::Runtime;
use r6::compiler::{Compiler, EnvVar, Syntax};
use r6::primitive::PRIM_ADD;
use r6::parser::Parser;

macro_rules! assert_evaluates_to {
    ($src:expr, $expected:expr) => (
        {
            let mut src_reader = BufReader::new($src.as_bytes());
            let mut src_parser = Parser::new(&mut src_reader);
            let sourcecode = match src_parser.parse_datum() {
                Ok(code) => code,
                Err(e) => panic!("failed to parse source: {:?}", e)
            };

            let mut res_reader = BufReader::new($expected.as_bytes());
            let mut res_parser = Parser::new(&mut res_reader);
            let expected = match res_parser.parse_datum() {
                Ok(val) => val,
                Err(e) => panic!("failed to parse result: {:?}", e)
            };

            let mut glob = HashMap::new();
            glob.insert(Cow::Borrowed("lambda"), EnvVar::Syntax(Syntax::Lambda));
            glob.insert(Cow::Borrowed("+"), EnvVar::PrimFunc("+", Rc::new(PRIM_ADD)));
            let mut compiler = Compiler::new(&glob);
            let bytecode = match compiler.compile(&sourcecode) {
                Ok(code) => code,
                Err(e) => panic!("compile failure: {:?}", e)
            };
            let mut runtime = Runtime::new(bytecode);
            let result = runtime.run();
            if !((result == expected) && (expected == result)) {
                panic!("test failed: expected `{:?}` but got `{:?}`", expected, result);
            }
        }
    )
}

#[test]
fn lexical_scoping() {
    // (\y f -> f 2) #f ((\y -> (\x -> y)) #t)
    // If it's dynamic scope, it should return 0
    // If it's static scope, it should return 1
    assert_evaluates_to!("((lambda (y f) (f 2)) #f ((lambda (y) (lambda (x) y)) #t))", "#t")
}
