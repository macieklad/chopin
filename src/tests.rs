#[cfg(test)]
mod tests {
    use crate::interpreter;
    use pest_string::parser::Error;
    use pest_string::StringParser;

    #[derive(Parser, StringParser)]
    #[grammar = "grammar.pest"]
    struct ChopinParser;

    fn evaluate(code: &str) -> Result<String, String> {
        match ChopinParser::parse_string(code.to_string()) {
            Ok(stmts) => {
                let mut interp = interpreter::Interpreter::default();
                let res = interp.interpret(&stmts);
                match res {
                    Ok(()) => Ok(interp.output.join("\n")),
                    Err(err) => Err(err),
                }
            }
            Err(err) => Err(format!("{:?}", err)),
        }
    }
    fn check_output(code: &str, expected_output: &str) {
        let res = evaluate(code);
        match res {
            Ok(output) => assert_eq!(output, expected_output),
            Err(err) => panic!("{}", err),
        }
    }
    fn check_error(code: &str, f: &dyn Fn(&str) -> ()) {
        let res = evaluate(code);
        match res {
            Ok(output) => panic!("{}", output),
            Err(err) => f(&err),
        }
    }
    #[test]
    fn test_factorial() {
        fn factorial(n: i32) -> i32 {
            if n <= 1 {
                return 1;
            }
            return n * factorial(n - 1);
        }
        check_output(
            "fun factorial(n) { \n\
                 if (n <= 1) {\n\
                     return 1; \n\
                 }\n\
                 return n * factorial(n - 1); \n\
               } \n\
               print factorial(10); ",
            &format!("{}", factorial(10)),
        )
    }
    #[test]
    fn test_invalid_binary_operands() {
        check_error("1 + [1,2,3];", &|err: &str| {
            assert!(err.starts_with("invalid operands in binary operator"))
        })
    }
    #[test]
    fn test_invalid_unary_operand() {
        check_error("-\"cat\";", &|err: &str| {
            assert!(
                err.starts_with("invalid application of unary op Minus to object of type String")
            )
        })
    }
    #[test]
    fn return_not_enclosed_in_fundecl() {
        check_error("return 1;", &|err: &str| {
            assert!(err.starts_with("return statement not enclosed in a FunDecl at"))
        })
    }
    #[test]
    fn test_clock() {
        evaluate("print clock();").unwrap();
    }
    #[test]
    fn test_for() {
        check_output(
            "for (var i = 0; i < 5; i = i + 1) \n\
               { \n\
                   print(i); \n\
               }",
            "0\n1\n2\n3\n4",
        );
    }
    #[test]
    fn test_chopin_funcs() {
        check_output(
            "fun sayHi(first, last) {\n\
                 return first + \", \" + last + \"!\";\n\
               }\n\
               \n\
               print sayHi(\"Hello\", \"World\");\n\
               \n\
               fun add(x,y,z) {\n\
                   return x + y + z;\n\
               }\n\
               \n\
               print add(1,2,3);",
            "'Hello, World!'\n6",
        )
    }
    #[test]
    fn test_implict_nil_return_1() {
        check_output(
            "fun f() { return; }\n\
               print f();",
            "nil",
        )
    }
    #[test]
    fn test_implict_nil_return_2() {
        check_output(
            "fun f() { }\n\
               print f();",
            "nil",
        )
    }

    #[test]
    fn test_implicit_nil_return_3() {
        check_output("fun f() {} print f();", "nil")
    }
    #[test]
    fn test_scopes() {
        check_output(
            "var a = \"global a\";\
      var b = \"global b\";\n\
      var c = \"global c\";\n\
      {
        var a = \"outer a\";\n\
        var b = \"outer b\";\n\
        {
          var a = \"inner a\";\n\
          print a;\n\
          print b;\n\
          print c;\n\
        }
        print a;\n\
        print b;\n\
        print c;\n\
      }
      print a;\n\
      print b;\n\
      print c;\n",
            "'inner a'\n\
      'outer b'\n\
      'global c'\n\
      'outer a'\n\
      'outer b'\n\
      'global c'\n\
      'global a'\n\
      'global b'\n\
      'global c'",
        )
    }
    #[test]
    fn test_closures_1() {
        check_output(
            "fun f(n) {\n\
                 var m = 2;\n\
                 fun g(p) {\n\
                   return p + m;\n\
                 }\n\
                 return g(n);\n\
               }\n\
               print f(1);",
            "3",
        )
    }
    #[test]
    fn test_closures_2() {
        check_output(
            "fun mkfun(n) {\n\
                 fun f(m) {\n\
                   return m + n;\n\
                   }\n\
                 return f;\n\
                 }\n\
               print mkfun(2)(3);",
            "5",
        )
    }
    #[test]
    fn test_classes_as_values() {
        check_output(
            "class DevonshireCream {\n\
                 serveOn() {\n\
                   return \"Scones\";\n\
                 }\n\
               }\n\
               \n\
               print DevonshireCream;",
            "ChopinClass(DevonshireCream)",
        )
    }
    #[test]
    fn test_setattr_1() {
        check_output(
            "class Foo {}\n\
               var foo = Foo();\n\
               foo.attr = 42;\n\
               print foo.attr;",
            "42",
        )
    }
    #[test]
    fn test_setattr_2() {
        check_output(
            "class Bar {}\n\
               class Foo {}\n\
               var foo = Foo();\n\
               foo.bar = Bar();\n\
               foo.bar.baz = \"baz\";\n\
               print foo.bar.baz;",
            "\'baz\'",
        )
    }
    #[test]
    fn test_methods_1() {
        check_output(
            "class Bacon {\
                  eat() {\
                    print \"Crunch crunch crunch!\";\
                  }\
                }\
                \
                Bacon().eat();",
            "\'Crunch crunch crunch!\'",
        )
    }
    #[test]
    fn test_method_this_binding_1() {
        check_output(
            "class Cake {\
                 taste() {\
                   var adjective = \"delicious\";\
                   print \"The \" + this.flavor + \" cake is \" + adjective + \"!\";\
                 }\
               }\
               \
               var cake = Cake();\
               cake.flavor = \"German chocolate\";\
               cake.taste();",
            "\'The German chocolate cake is delicious!\'",
        )
    }
    #[test]
    fn test_method_this_binding_2() {
        check_output(
            "class Thing {\
                 getCallback() {\
                   fun localFunction() {\
                     print this;\
                   }\
                   \
                   return localFunction;\
                 }\
               }\
               \
               var callback = Thing().getCallback();\
               callback();",
            "ChopinInstance(Thing)",
        )
    }
    #[test]
    fn test_method_this_binding_3() {
        check_output(
            "class Foo {\n
                 init(x) {\n\
                   this.x = x;\n\
                 }\n\
                 getX() {\n\
                   return this.x;\n\
                 }\n\
               }\n\
               \n\
               var foo = Foo(42);
               print foo.getX();",
            "42",
        )
    }
    #[test]
    fn test_init_1() {
        check_output(
            "class Foo {\
                 init(val) {\
                   this.val = val;\
                 }\
               }\
               \
               var foo = Foo(42);\
               print foo.val;",
            "42",
        )
    }
    #[test]
    fn test_explicit_call_init() {
        check_output(
            "class Foo {\
                 init(val) {\
                   this.val = val;\
                 }\
               }\
               \
               var foo1 = Foo(42);\
               print foo1.val;\
               var foo2 = foo1.init(1337);\
               print foo2.val;\
               print foo1.val;",
            "42\n1337\n1337",
        )
    }
    #[test]
    fn test_early_return_init() {
        check_output(
            "class Foo {\n\
                 init(val) {\n\
                   if (val > 100) {\n\
                     this.val = 100;\n\
                     return;\n\
                   }\n\
                   this.val = val;\n\
                 }\n\
               }\n\
               \n\
               var foo1 = Foo(42);\n\
               print foo1.val;\n\
               var foo2 = Foo(200);\n\
               print foo2.val;",
            "42\n100",
        )
    }
    #[test]
    fn test_return_non_nil_in_init() {
        check_error(
            "class Foo {\n\
                 init(val) {\n\
                   return 42;\n\
                 }\n\
               }\n\
               \n\
               var foo = Foo(42);",
            &|err: &str| {
                assert_eq!(
                    err,
                    "TypeError: init should only return nil (perhaps implicitly), not Number"
                )
            },
        )
    }
    #[test]
    fn class_cannot_inherit_from_itself() {
        check_error("class Oops < Oops {}", &|err: &str| {
            assert!(err.starts_with("A class cannot inerit from itself"))
        })
    }
    #[test]
    fn only_classes_can_be_superclasses() {
        check_error("var x = 42; class Oops < x {}", &|err: &str| {
            assert!(err.starts_with("Only classes should appear as superclasses."))
        })
    }
    #[test]
    fn method_inheritance_1() {
        check_output(
            "class A {\n\
                 f() {\n\
                   return \"cat\";\n\
                 }\n\
               }\n\
               class B < A {}\n\
               var b = B();\n\
               print b.f();",
            "\'cat\'",
        )
    }
    #[test]
    fn method_inheritance_2() {
        check_output(
            "class A {\n\
                 f() {\n\
                   return \"cat\";\n\
                 }\n\
               }\n\
               class B < A {}\n\
               class C < B {}\n\
               var c = C();\n\
               print c.f();",
            "\'cat\'",
        )
    }
    #[test]
    fn method_inheritance_3() {
        check_output(
            "class A {\n\
                 f() {\n\
                   return this.attr;
                 }\n\
               }\n\
               class B < A {\n\
                 init(attr) {\n\
                   this.attr = attr;\n\
                 }\n\
               }\n\
               var b = B(42);\n\
               print b.f();",
            "42",
        )
    }
    #[test]
    fn method_inheritance_4() {
        check_output(
            "class A {\n\
                 f() {\n\
                   return this.attr;
                 }\n\
               }\n\
               class B < A {\n\
               }\n\
               var b = B();\n\
               b.attr = 42;
               print b.f();",
            "42",
        )
    }
    #[test]
    fn illegal_super_expressions_1() {
        check_error("super + 1", &|err: &str| {
            assert!(err.starts_with("Expected token Dot"))
        })
    }
    #[test]
    fn illegal_super_expressions_2() {
        check_error("fun f() { return super.g(); }\nprint f();", &|err: &str| {
            assert!(err.starts_with("Super expression not enclosed in a method definition"))
        })
    }
    #[test]
    fn test_super_1() {
        check_output(
            "class A {\n\
                 method() {\n\
                   print \"A method\";\n\
                 }\n\
               }\n\
               \n\
               class B < A {\n\
                 method() {\n\
                   print \"B method\";\n\
                 }\n\
                 \n\
                 test() {\n\
                   super.method();\n\
                 }\n\
               }\n\
               \n\
               class C < B {}\n\
               \n\
               C().test();",
            "'A method'",
        )
    }
    #[test]
    fn test_super_2() {
        check_output(
            "class A {\n\
                 method() {\n\
                   print \"A method\";\n\
                 }\n\
               }\n\
               \n\
               class B < A {\n\
                 method() {\n\
                   print \"B method\";\n\
                 }\n\
                 \n\
                 test() {\n\
                   var method = super.method;\n\
                   method();\n\
                 }\n\
               }\n\
               \n\
               class C < B {}\n\
               \n\
               C().test();",
            "'A method'",
        )
    }
    #[test]
    fn test_super_3() {
        check_output(
            "class A {\n\
                 f() {\n\
                   return this.attr;
                 }\n\
               }\n\
               class B < A {\n\
                 init(attr) {\n\
                   this.attr = attr;\n\
                 }\n\
                 f() {\n\
                   return 1337;
                 }\n\
                 g() {\n\
                   return super.f();\n\
                 }\n\
               }\n\
               var b = B(42);\n\
               print b.g();",
            "42",
        )
    }
    #[test]
    fn test_late_binding() {
        check_output(
            "fun a() { b(); }\n\
               fun b() { print \"hello world\"; }\n\
               \n\
               a();\n",
            "'hello world'",
        )
    }
    #[test]
    fn test_list_construction() {
        check_output("print([1,2,3]);", "[1, 2, 3]")
    }
    #[test]
    fn test_empty_list_construction() {
        check_output("print([]);", "[]")
    }
    #[test]
    fn test_list_concat() {
        check_output("print([1,2,3] + [4,5,6]);", "[1, 2, 3, 4, 5, 6]")
    }
    #[test]
    fn test_len() {
        check_output(
            "print(len(\"\")); \n\
               print(len(\"cat\")); \n\
               print(len([])); \n\
               print(len([1,2,3,4]));",
            "0\n3\n0\n4",
        )
    }
    #[test]
    fn test_for_each() {
        check_output(
            "fun f(arg) { print arg; } \n\
               forEach([1,2,3,4], f);",
            "1\n2\n3\n4",
        )
    }
    #[test]
    fn test_map() {
        check_output(
            "fun incr(x) { return x + 1; } \n\
               print(map(incr, [1,2,3,4]));",
            "[2, 3, 4, 5]",
        )
    }
    #[test]
    fn test_list_subscripts() {
        check_output(
            "var xs = [0,1]; \n\
               print(xs[0]); \n\
               print(xs[1]); \n\
               print(xs[-1]); \n\
               print(xs[-2]); \n\
               ",
            "0\n1\n1\n0",
        )
    }
}
