use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::expr;
use std::fmt;

static INIT: &str = "init";

trait Callable {
  fn arity(&self, interpreter: &Interpreter) -> u8;
  fn call(&self, interpreter: &mut Interpreter, args: &[Value]) -> Result<Value, String>;
}

#[derive(Clone)]
pub struct NativeFunction {
  pub name: String,
  pub arity: u8,
  pub callable: fn(&mut Interpreter, &[Value]) -> Result<Value, String>,
}

impl fmt::Debug for NativeFunction {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "NativeFunction({})", self.name)
  }
}

impl Callable for NativeFunction {
  fn arity(&self, _interpreter: &Interpreter) -> u8 {
    self.arity
  }
  fn call(&self, interpreter: &mut Interpreter, args: &[Value]) -> Result<Value, String> {
    (self.callable)(interpreter, args)
  }
}

#[derive(Clone, Debug)]
pub struct ChopinFunction {
  pub id: u64,
  pub name: expr::Symbol,
  pub parameters: Vec<expr::Symbol>,
  pub body: Vec<expr::Stmt>,
  pub closure: Environment,
  pub this_binding: Option<Box<Value>>,
  pub superclass: Option<u64>,
  pub is_initializer: bool,
}

impl Callable for ChopinFunction {
  fn arity(&self, _interpreter: &Interpreter) -> u8 {
    self.parameters.len().try_into().unwrap()
  }
  fn call(&self, interpreter: &mut Interpreter, args: &[Value]) -> Result<Value, String> {
    let args_env: HashMap<_, _> = self
      .parameters
      .iter()
      .zip(args.iter())
      .map(|(param, arg)| {
        (
          param.name.clone(),
          (
            Some(arg.clone()),
            SourceLocation {
              line: param.line,
              col: param.col,
            },
          ),
        )
      })
      .collect();

    let saved_env = interpreter.env.clone();
    let saved_retval = interpreter.retval.clone();
    let saved_enclosing_function = interpreter.enclosing_function;

    let mut env = self.closure.clone();
    env.venv.extend(saved_env.venv.clone());
    env.venv.extend(args_env);

    if let Some(this_val) = &self.this_binding {
      // this is just used for lookup on name. source location is meaningless and unused`
      let this_symbol = Interpreter::this_symbol(0, -1);
      env.venv.insert(
        this_symbol.name,
        (
          Some(*this_val.clone()),
          SourceLocation {
            line: this_symbol.line,
            col: this_symbol.col,
          },
        ),
      );
    } else if let Ok(this_val) = interpreter.lookup(&Interpreter::this_symbol(0, -1)) {
      // this is just used for lookup on name. source location is meaningless and unused`
      let this_symbol = Interpreter::this_symbol(0, -1);
      env.venv.insert(
        this_symbol.name,
        (
          Some(this_val.clone()),
          SourceLocation {
            line: this_symbol.line,
            col: this_symbol.col,
          },
        ),
      );
    }
    let env = env;

    interpreter.env = env;
    interpreter.enclosing_function = Some(self.id);
    interpreter.backtrace.push((0, self.name.name.clone()));
    interpreter.interpret(&self.body)?;

    let retval = interpreter.retval.clone();
    interpreter.backtrace.pop();
    interpreter.enclosing_function = saved_enclosing_function;
    interpreter.env = saved_env;
    interpreter.retval = saved_retval;

    match retval {
      Some(val) => {
        let val_type = type_of(&val);
        if self.is_initializer && val_type != Type::Nil {
          Err(format!(
            "TypeError: init should only return nil (perhaps implicitly), not {:?}",
            val_type
          ))
        } else {
          Ok(val)
        }
      }
      None => {
        if self.is_initializer {
          match &self.this_binding {
            Some(this_val) => Ok(*this_val.clone()),
            None => panic!("Internal intepreter error: could not find binding for this."),
          }
        } else {
          Ok(Value::Nil)
        }
      }
    }
  }
}

#[derive(Clone, Debug)]
pub struct ChopinClass {
  pub name: expr::Symbol,
  pub superclass: Option<u64>,
  pub id: u64,
  pub methods: HashMap<String, u64>,
}

impl Callable for ChopinClass {
  fn arity(&self, interpreter: &Interpreter) -> u8 {
    match self.init(interpreter) {
      Some(initializer) => initializer.parameters.len().try_into().unwrap(),
      None => 0,
    }
  }
  fn call(&self, interpreter: &mut Interpreter, args: &[Value]) -> Result<Value, String> {
    let instance = interpreter.create_instance(&self.name, self.id);

    if let Some(mut initializer) = self.init(&interpreter) {
      initializer.this_binding = Some(Box::new(instance.clone()));
      initializer.call(interpreter, args)?;
    }

    Ok(instance)
  }
}

impl ChopinClass {
  fn init(&self, interpreter: &Interpreter) -> Option<ChopinFunction> {
    match self.methods.get(&String::from(INIT)) {
      Some(initializer_id) => match interpreter.chopin_functions.get(initializer_id) {
        Some(initializer) => Some(initializer.clone()),
        None => panic!(
          "Internal interpreter error! couldn't find an initializer method with id {}.",
          initializer_id
        ),
      },
      None => None,
    }
  }

  fn find_method(
    &self,
    method_name: &str,
    interpreter: &Interpreter,
  ) -> Option<(expr::Symbol, u64)> {
    if let Some(method_id) = self.methods.get(method_name) {
      if let Some(chopin_fn) = interpreter.chopin_functions.get(method_id) {
        return Some((chopin_fn.name.clone(), *method_id));
      }
      panic!(
        "Internal interpreter error! Could not find Chopin fn with id {}.",
        method_id
      );
    } else if let Some(superclass_id) = self.superclass {
      if let Some(superclass) = interpreter.chopin_classes.get(&superclass_id) {
        return superclass.find_method(method_name, interpreter);
      }
      panic!(
        "Internal interpreter error! Could not find Chopin fn with id {}.",
        superclass_id
      )
    }
    None
  }
}

#[derive(Clone, Debug)]
pub struct ChopinInstance {
  pub class_name: expr::Symbol,
  pub class_id: u64,
  pub id: u64,
  pub fields: HashMap<String, Value>,
}

impl ChopinInstance {
  fn getattr(&self, attr: &str, interpreter: &Interpreter) -> Result<Value, String> {
    match self.fields.get(attr) {
      Some(val) => Ok(val.clone()),
      None => {
        if let Some(cls) = interpreter.chopin_classes.get(&self.class_id) {
          if let Some((func_name, method_id)) = cls.find_method(attr, interpreter) {
            return Ok(Value::ChopinFunction(
              func_name,
              method_id,
              Some(Box::new(Value::ChopinInstance(
                self.class_name.clone(),
                self.id,
              ))),
            ));
          }
          Err(format!(
            "AttributeError: '{}' instance has no '{}' attribute.",
            self.class_name.name, attr
          ))
        } else {
          panic!(
            "Internal interpreter error! Could not find class with id {}",
            self.class_id
          );
        }
      }
    }
  }
}

#[derive(Debug, Clone)]
pub enum Value {
  Number(f64),
  String(String),
  Bool(bool),
  Nil,
  NativeFunction(NativeFunction),
  ChopinFunction(
    expr::Symbol,
    /*id*/ u64,
    /*this binding*/ Option<Box<Value>>,
  ),
  ChopinClass(expr::Symbol, /*id*/ u64),
  ChopinInstance(expr::Symbol, /*id*/ u64),
  List(Vec<Value>),
}

fn as_callable(interpreter: &Interpreter, value: &Value) -> Option<Box<dyn Callable>> {
  match value {
    Value::NativeFunction(f) => Some(Box::new(f.clone())),
    Value::ChopinFunction(_, id, this_binding) => match interpreter.chopin_functions.get(id) {
      Some(f) => {
        let mut f_copy = f.clone();
        f_copy.this_binding = this_binding.clone();
        Some(Box::new(f_copy))
      }
      None => panic!(
        "Internal interpreter error! Could not find ChopinFunction with id {}.",
        id
      ),
    },
    Value::ChopinClass(_, id) => match interpreter.chopin_classes.get(id) {
      Some(cls) => Some(Box::new(cls.clone())),
      None => panic!(
        "Internal interpreter error! Could not find ChopinClass with id {}.",
        id
      ),
    },
    _ => None,
  }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Type {
  Number,
  String,
  Bool,
  Nil,
  NativeFunction,
  ChopinFunction,
  ChopinClass,
  ChopinInstance,
  List,
}

pub fn type_of(val: &Value) -> Type {
  match val {
    Value::Number(_) => Type::Number,
    Value::String(_) => Type::String,
    Value::Bool(_) => Type::Bool,
    Value::Nil => Type::Nil,
    Value::NativeFunction(_) => Type::NativeFunction,
    Value::ChopinFunction(_, _, _) => Type::ChopinFunction,
    Value::ChopinClass(_, _) => Type::ChopinClass,
    Value::ChopinInstance(_, _) => Type::ChopinInstance,
    Value::List(_) => Type::List,
  }
}

impl fmt::Display for Value {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match &self {
      Value::Number(n) => write!(f, "{}", n),
      Value::String(s) => write!(f, "'{}'", s),
      Value::Bool(b) => write!(f, "{}", b),
      Value::Nil => write!(f, "nil"),
      Value::NativeFunction(func) => write!(f, "NativeFunction({})", func.name),
      Value::ChopinFunction(sym, _, _) => write!(f, "ChopinFunction({})", sym.name),
      Value::ChopinClass(sym, _) => write!(f, "ChopinClass({})", sym.name),
      Value::ChopinInstance(sym, _) => write!(f, "ChopinInstance({})", sym.name),
      Value::List(elements) => {
        write!(f, "[")?;
        elements.split_last().map(|(last_elt, rest)| {
          rest.iter().try_for_each(|elt| write!(f, "{}, ", elt))?;
          write!(f, "{}", last_elt)
        });
        write!(f, "]")
      }
    }
  }
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
  line: usize,
  col: i64,
}

#[derive(Debug, Default, Clone)]
pub struct Environment {
  enclosing: Option<Box<Environment>>,
  // SourceLocation is the location of a declaration
  venv: HashMap<String, (Option<Value>, SourceLocation)>,
}

pub enum LookupResult<'a> {
  Ok(&'a Value),
  UndefButDeclared(SourceLocation),
  UndefAndNotDeclared,
}

impl Environment {
  pub fn with_enclosing(enclosing: Environment) -> Environment {
    Environment {
      enclosing: Some(Box::new(enclosing)),
      venv: HashMap::new(),
    }
  }

  pub fn define(&mut self, sym: expr::Symbol, maybe_val: Option<Value>) {
    self.venv.insert(
      sym.name,
      (
        maybe_val,
        SourceLocation {
          line: sym.line,
          col: sym.col,
        },
      ),
    );
  }

  pub fn lookup(&self, sym: &expr::Symbol) -> LookupResult {
    match self.venv.get(&sym.name) {
      Some((maybe_val, defn_source_location)) => match maybe_val {
        Some(val) => LookupResult::Ok(&val),
        None => LookupResult::UndefButDeclared(SourceLocation {
          line: defn_source_location.line,
          col: defn_source_location.col,
        }),
      },
      None => LookupResult::UndefAndNotDeclared,
    }
  }

  pub fn get(&self, sym: &expr::Symbol) -> Result<&Value, String> {
    match self.lookup(&sym) {
      LookupResult::Ok(val) => Ok(&val),
      LookupResult::UndefButDeclared(source_location) => Err(format!(
        "Use of undefined variable '{}' at line={},col={}.\
                \nNote: {} was previously declared at line={},col={}, \
                but was never defined.",
        &sym.name, sym.line, sym.col, &sym.name, source_location.line, source_location.col
      )),
      LookupResult::UndefAndNotDeclared => match &self.enclosing {
        Some(enclosing) => enclosing.get(sym),
        None => Err(format!(
          "Use of undefined variable {} at line={},col={}.\nNote: {} was never declared.",
          &sym.name, sym.line, sym.col, &sym.name
        )),
      },
    }
  }

  pub fn assign(&mut self, sym: expr::Symbol, val: &Value) -> Result<(), String> {
    if self.venv.contains_key(&sym.name) {
      self.define(sym, Some(val.clone()));
      return Ok(());
    }

    match &mut self.enclosing {
      Some(enclosing) => enclosing.assign(sym, val),
      None => Err(format!(
        "attempting to assign to undeclared variable at line={},col={}",
        sym.line, sym.col
      )),
    }
  }
}

pub struct Interpreter {
  pub counter: u64,
  pub chopin_functions: HashMap<u64, ChopinFunction>,
  pub chopin_instances: HashMap<u64, ChopinInstance>,
  pub chopin_classes: HashMap<u64, ChopinClass>,
  pub env: Environment,
  pub globals: Environment,
  pub retval: Option<Value>,
  pub output: Vec<String>,
  pub enclosing_function: Option<u64>,
  pub interrupted: Arc<AtomicBool>,
  pub backtrace: Vec<(u64, String)>,
}

impl Default for Interpreter {
  fn default() -> Interpreter {
    let mut globals_venv = HashMap::new();
    globals_venv.insert(
      String::from("clock"),
      (
        Some(Value::NativeFunction(NativeFunction {
          name: String::from("clock"),
          arity: 0,
          callable: |_, _| {
            let start = SystemTime::now();
            let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();

            Ok(Value::Number(since_the_epoch.as_millis() as f64))
          },
        })),
        SourceLocation {
          line: 1337,
          col: 1337,
        },
      ),
    );
    globals_venv.insert(
      String::from("len"),
      (
        Some(Value::NativeFunction(NativeFunction {
          name: String::from("len"),
          arity: 1,
          callable: |_, values| match &values[0] {
            Value::String(s) => Ok(Value::Number(s.len() as f64)),
            Value::List(elts) => Ok(Value::Number(elts.len() as f64)),
            val => Err(format!("Object of type {:?} has no len.", type_of(val))),
          },
        })),
        SourceLocation {
          line: 1337,
          col: 1337,
        },
      ),
    );
    globals_venv.insert(
      String::from("forEach"),
      (
        Some(Value::NativeFunction(NativeFunction {
          name: String::from("forEach"),
          arity: 2,
          callable: |interpreter, values| match &values[0] {
            Value::List(elts) => {
              let maybe_callable = as_callable(interpreter, &values[1]);
              match maybe_callable {
                Some(callable) => {
                  for elt in elts {
                    callable.call(interpreter, &[elt.clone()])?;
                  }
                  Ok(Value::Nil)
                }
                None => Err(format!(
                  "The second argument to for_each must be callable. Found {:?}.",
                  type_of(&values[1])
                )),
              }
            }
            val => Err(format!(
              "Can't call forEach on value of type {:?}.",
              type_of(val)
            )),
          },
        })),
        SourceLocation {
          line: 1337,
          col: 1337,
        },
      ),
    );
    globals_venv.insert(
      String::from("map"),
      (
        Some(Value::NativeFunction(NativeFunction {
          name: String::from("map"),
          arity: 2,
          callable: |interpreter, values| match &values[1] {
            Value::List(elts) => {
              let maybe_callable = as_callable(interpreter, &values[0]);
              match maybe_callable {
                Some(callable) => {
                  let mut res = Vec::new();
                  for elt in elts {
                    res.push(callable.call(interpreter, &[elt.clone()])?);
                  }
                  Ok(Value::List(res))
                }
                None => Err(format!(
                  "The second argument to for_each must be callable. Found {:?}.",
                  type_of(&values[1])
                )),
              }
            }
            val => Err(format!(
              "Can't call forEach on value of type {:?}.",
              type_of(val)
            )),
          },
        })),
        SourceLocation {
          line: 1337,
          col: 1337,
        },
      ),
    );

    let globals = Environment {
      enclosing: None,
      venv: globals_venv,
    };

    Interpreter {
      counter: 0,
      chopin_functions: Default::default(),
      chopin_instances: Default::default(),
      chopin_classes: Default::default(),
      env: Default::default(),
      globals,
      retval: None,
      output: Default::default(),
      enclosing_function: None,
      interrupted: Arc::new(AtomicBool::new(false)),
      backtrace: vec![(0, "script".to_string())],
    }
  }
}

impl Interpreter {
  pub fn interpret(&mut self, stmts: &[expr::Stmt]) -> Result<(), String> {
    self.interrupted.store(false, Ordering::Release);
    for stmt in stmts {
      self.execute(stmt)?
    }
    Ok(())
  }

  pub fn format_backtrace(&self) -> String {
    let lines: Vec<_> = self
      .backtrace
      .iter()
      .map(|(_, funname)| format!("[line ??] in {}", funname))
      .collect();
    format!("Backtrace (most recent call last):\n\n{}", lines.join("\n"))
  }

  fn alloc_id(&mut self) -> u64 {
    let res = self.counter;
    self.counter += 1;
    res
  }

  fn create_instance(&mut self, class_name: &expr::Symbol, class_id: u64) -> Value {
    let inst_id = self.alloc_id();
    let inst = ChopinInstance {
      class_name: class_name.clone(),
      class_id,
      id: inst_id,
      fields: HashMap::new(),
    };
    self.chopin_instances.insert(inst_id, inst);
    Value::ChopinInstance(class_name.clone(), inst_id)
  }

  fn execute(&mut self, stmt: &expr::Stmt) -> Result<(), String> {
    if self.retval.is_some() {
      return Ok(());
    }

    match stmt {
      expr::Stmt::Expr(e) => match self.interpret_expr(e) {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
      },
      expr::Stmt::ClassDecl(expr::ClassDecl {
        name: sym,
        superclass: maybe_superclass,
        methods: stmt_methods,
      }) => {
        let class_id = self.alloc_id();
        self
          .env
          .define(sym.clone(), Some(Value::ChopinClass(sym.clone(), class_id)));

        let superclass_id = if let Some(superclass_var) = maybe_superclass {
          if superclass_var.name == sym.name {
            return Err(format!(
              "A class cannot inerit from itself (line={}, col={})",
              sym.line, sym.col
            ));
          }

          let superclass_val =
            self.interpret_expr(&expr::Expr::Variable(superclass_var.clone()))?;
          if let Value::ChopinClass(_, id) = superclass_val {
            Some(id)
          } else {
            return Err(format!(
              "Only classes should appear as superclasses. Found {:?}.",
              type_of(&superclass_val)
            ));
          }
        } else {
          None
        };

        let mut methods = HashMap::new();
        for method in stmt_methods.iter() {
          let func_id = self.alloc_id();

          methods.insert(method.name.name.clone(), func_id);

          let is_initializer = method.name.name == INIT;

          let chopin_function = ChopinFunction {
            id: func_id,
            name: method.name.clone(),
            parameters: method.params.clone(),
            body: method.body.clone(),
            closure: self.env.clone(),
            this_binding: None,
            superclass: superclass_id,
            is_initializer,
          };

          self.chopin_functions.insert(func_id, chopin_function);
        }

        let cls = ChopinClass {
          name: sym.clone(),
          superclass: superclass_id,
          id: class_id,
          methods,
        };

        self.chopin_classes.insert(class_id, cls);
        Ok(())
      }
      expr::Stmt::FunDecl(expr::FunDecl {
        name,
        params: parameters,
        body,
      }) => {
        let func_id = self.alloc_id();
        self.env.define(
          name.clone(),
          Some(Value::ChopinFunction(name.clone(), func_id, None)),
        );

        let chopin_function = ChopinFunction {
          id: func_id,
          name: name.clone(),
          parameters: parameters.clone(),
          body: body.clone(),
          closure: self.env.clone(),
          this_binding: None,
          superclass: None,
          is_initializer: false,
        };

        self.chopin_functions.insert(func_id, chopin_function);

        Ok(())
      }
      expr::Stmt::If(cond, if_true, maybe_if_false) => {
        if Interpreter::is_truthy(&self.interpret_expr(cond)?) {
          return self.execute(if_true);
        }
        if let Some(if_false) = maybe_if_false {
          return self.execute(if_false);
        }
        Ok(())
      }
      expr::Stmt::Print(e) => match self.interpret_expr(e) {
        Ok(val) => {
          println!("{}", val);
          self.output.push(format!("{}", val));
          Ok(())
        }
        Err(err) => Err(err),
      },
      expr::Stmt::VarDecl(sym, maybe_expr) => {
        let maybe_val = match maybe_expr {
          Some(expr) => Some(self.interpret_expr(expr)?),
          None => None,
        };
        self.env.define(sym.clone(), maybe_val);
        Ok(())
      }
      expr::Stmt::Block(stmts) => {
        self.env = Environment::with_enclosing(self.env.clone());

        for stmt in stmts.iter() {
          self.execute(stmt)?;
        }

        if let Some(enclosing) = self.env.enclosing.clone() {
          self.env = *enclosing
        } else {
          // TODO: how to do this without a runtime check?
          panic!("impossible");
        }

        Ok(())
      }
      expr::Stmt::While(cond, body) => {
        while Interpreter::is_truthy(&self.interpret_expr(cond)?) {
          self.execute(body)?;
        }
        Ok(())
      }
      expr::Stmt::Return(_, maybe_res) => {
        self.retval = Some(if let Some(res) = maybe_res {
          self.interpret_expr(res)?
        } else {
          Value::Nil
        });
        Ok(())
      }
    }
  }

  fn lookup(&self, sym: &expr::Symbol) -> Result<&Value, String> {
    match self.env.get(sym) {
      Ok(val) => Ok(val),
      Err(_) => self.globals.get(sym),
    }
  }

  fn this_symbol(line: usize, col: i64) -> expr::Symbol {
    expr::Symbol {
      name: String::from("this"),
      line,
      col,
    }
  }

  fn interpret_expr(&mut self, expr: &expr::Expr) -> Result<Value, String> {
    if self.interrupted.load(Ordering::Acquire) {
      return Ok(Value::Nil);
    }

    match expr {
      expr::Expr::This(source_location) => match self.lookup(&Interpreter::this_symbol(
        source_location.line,
        source_location.col,
      )) {
        Ok(val) => Ok(val.clone()),
        Err(err) => Err(err),
      },
      expr::Expr::Literal(lit) => Ok(Interpreter::interpret_literal(lit)),
      expr::Expr::Unary(op, e) => self.interpret_unary(*op, e),
      expr::Expr::Binary(lhs, op, rhs) => self.interpret_binary(lhs, *op, rhs),
      expr::Expr::Call(callee, loc, args) => self.call(callee, loc, args),
      expr::Expr::Get(lhs, attr) => self.getattr(lhs, &attr.name),
      expr::Expr::Set(lhs, attr, rhs) => self.setattr(lhs, attr, rhs),
      expr::Expr::Grouping(e) => self.interpret_expr(e),
      expr::Expr::Variable(sym) => match self.lookup(sym) {
        Ok(val) => Ok(val.clone()),
        Err(err) => Err(err),
      },
      expr::Expr::Assign(sym, val_expr) => {
        let val = self.interpret_expr(val_expr)?;

        if let Err(err) = self.env.assign(sym.clone(), &val) {
          return Err(err);
        }

        Ok(val)
      }
      expr::Expr::Logical(left_expr, expr::LogicalOp::Or, right_expr) => {
        let left = self.interpret_expr(left_expr)?;
        if Interpreter::is_truthy(&left) {
          Ok(left)
        } else {
          Ok(self.interpret_expr(right_expr)?)
        }
      }
      expr::Expr::Logical(left_expr, expr::LogicalOp::And, right_expr) => {
        let left = self.interpret_expr(left_expr)?;
        if !Interpreter::is_truthy(&left) {
          Ok(left)
        } else {
          Ok(self.interpret_expr(right_expr)?)
        }
      }
      expr::Expr::Super(source_location, sym) => match self.enclosing_function {
        Some(func_id) => match self.chopin_functions.get(&func_id) {
          Some(func) => match &func.superclass {
            Some(superclass_id) => {
              if let Some(superclass) = self.chopin_classes.get(&superclass_id) {
                if let Some((func_name, method_id)) = superclass.find_method(&sym.name, &self) {
                  if let Some(method) = self.chopin_functions.get(&method_id) {
                    Ok(Value::ChopinFunction(
                      func_name,
                      method.id,
                      func.this_binding.clone(),
                    ))
                  } else {
                    panic!(
                      "Internal interpreter error! Could not find method with id {}.",
                      method_id
                    )
                  }
                } else {
                  Err(format!(
                    "no superclass has method {} at line={}, col={}",
                    sym.name, source_location.line, source_location.col
                  ))
                }
              } else {
                panic!(
                  "Internal interpreter error! Couldn't find class with id {}",
                  superclass_id
                )
              }
            }
            _ => Err(format!(
              "Super expression not enclosed in a method definition at line={}, col={}.",
              source_location.line, source_location.col
            )),
          },
          None => panic!(
            "Internal interpreter error! Couldn't find func with id {}.",
            func_id
          ),
        },
        None => Err(format!(
          "super expression not enclosed in a function at line={}, col={}.",
          source_location.line, source_location.col
        )),
      },
      expr::Expr::List(elements) => self.list(elements),
      expr::Expr::Subscript {
        value,
        slice,
        source_location,
      } => self.subscript(value, slice, source_location),
    }
  }

  fn subscript(
    &mut self,
    value_expr: &expr::Expr,
    slice_expr: &expr::Expr,
    source_location: &expr::SourceLocation,
  ) -> Result<Value, String> {
    let value = self.interpret_expr(value_expr)?;
    let value_type = type_of(&value);
    let slice = self.interpret_expr(slice_expr)?;
    if let Value::List(elements) = value {
      if let Value::Number(index_float) = slice {
        let index_int = index_float as i64;
        if 0 <= index_int && index_int < elements.len() as i64 {
          return Ok(elements[index_int as usize].clone());
        }
        if index_int < 0 && -index_int <= elements.len() as i64 {
          return Ok(elements[(elements.len() as i64 + index_int) as usize].clone());
        }
        Err(format!(
          "List subscript index out of range at {:?}",
          source_location
        ))
      } else {
        Err(format!(
          "Invalid subscript of type {:?} in subscript expression",
          value_type,
        ))
      }
    } else {
      Err(format!(
        "Invalid value of type {:?} in subscript expr.",
        type_of(&value),
      ))
    }
  }

  fn list(&mut self, element_exprs: &[expr::Expr]) -> Result<Value, String> {
    let maybe_elements: Result<Vec<_>, _> = element_exprs
      .iter()
      .map(|expr| self.interpret_expr(expr))
      .collect();

    match maybe_elements {
      Ok(args) => Ok(Value::List(args)),
      Err(err) => Err(err),
    }
  }

  fn getattr(&mut self, lhs: &expr::Expr, attr: &str) -> Result<Value, String> {
    let val = self.interpret_expr(lhs)?;
    match val {
      Value::ChopinInstance(_, id) => match self.chopin_instances.get(&id) {
        Some(inst) => inst.getattr(&attr, &self),
        None => panic!(
          "Internal interpreter error: could not find an instance with id {}.",
          id
        ),
      },
      _ => Err(format!(
        "Only ChopinInstance values have attributes. Found {:?}.",
        type_of(&val)
      )),
    }
  }

  fn setattr(
    &mut self,
    lhs_exp: &expr::Expr,
    attr: &expr::Symbol,
    rhs_exp: &expr::Expr,
  ) -> Result<Value, String> {
    let lhs = self.interpret_expr(lhs_exp)?;
    let rhs = self.interpret_expr(rhs_exp)?;
    match lhs {
      Value::ChopinInstance(_, id) => match self.chopin_instances.get_mut(&id) {
        Some(inst) => {
          inst.fields.insert(attr.name.clone(), rhs.clone());
          Ok(rhs)
        }
        None => panic!(
          "Internal interpreter error: could not find instance with id {}",
          id
        ),
      },
      _ => Err(format!(
        "Only ChopinInstance values have attributes. Found {:?}.",
        type_of(&lhs)
      )),
    }
  }

  fn call(
    &mut self,
    callee_expr: &expr::Expr,
    loc: &expr::SourceLocation,
    arg_exprs: &[expr::Expr],
  ) -> Result<Value, String> {
    let callee = self.interpret_expr(callee_expr)?;

    match as_callable(&self, &callee) {
      Some(callable) => {
        let maybe_args: Result<Vec<_>, _> = arg_exprs
          .iter()
          .map(|arg| self.interpret_expr(arg))
          .collect();

        match maybe_args {
          Ok(args) => {
            if args.len() != callable.arity(self).into() {
              Err(format!(
                "Invalid call at line={},col={}: callee has arity {}, but \
                                         was called with {} arguments",
                loc.line,
                loc.col,
                callable.arity(self),
                args.len()
              ))
            } else {
              callable.call(self, &args)
            }
          }
          Err(err) => Err(err),
        }
      }
      None => Err(format!(
        "value {:?} is not callable at line={},col={}",
        callee, loc.line, loc.col
      )),
    }
  }

  fn interpret_binary(
    &mut self,
    lhs_expr: &expr::Expr,
    op: expr::BinaryOp,
    rhs_expr: &expr::Expr,
  ) -> Result<Value, String> {
    let lhs = self.interpret_expr(lhs_expr)?;
    let rhs = self.interpret_expr(rhs_expr)?;

    match (&lhs, op.ty, &rhs) {
      (Value::Number(n1), expr::BinaryOpTy::Less, Value::Number(n2)) => Ok(Value::Bool(n1 < n2)),
      (Value::Number(n1), expr::BinaryOpTy::LessEqual, Value::Number(n2)) => {
        Ok(Value::Bool(n1 <= n2))
      }
      (Value::Number(n1), expr::BinaryOpTy::Greater, Value::Number(n2)) => Ok(Value::Bool(n1 > n2)),
      (Value::Number(n1), expr::BinaryOpTy::GreaterEqual, Value::Number(n2)) => {
        Ok(Value::Bool(n1 >= n2))
      }
      (Value::Number(n1), expr::BinaryOpTy::Plus, Value::Number(n2)) => Ok(Value::Number(n1 + n2)),
      (Value::Number(n1), expr::BinaryOpTy::Minus, Value::Number(n2)) => Ok(Value::Number(n1 - n2)),
      (Value::Number(n1), expr::BinaryOpTy::Star, Value::Number(n2)) => Ok(Value::Number(n1 * n2)),
      (Value::Number(n1), expr::BinaryOpTy::Slash, Value::Number(n2)) => {
        if *n2 != 0.0 {
          Ok(Value::Number(n1 / n2))
        } else {
          Err(format!(
            "division by zero at line={},col={}",
            op.line, op.col
          ))
        }
      }
      (Value::String(s1), expr::BinaryOpTy::Plus, Value::String(s2)) => {
        Ok(Value::String(format!("{}{}", s1, s2)))
      }
      (Value::List(xs), expr::BinaryOpTy::Plus, Value::List(ys)) => {
        let mut res = xs.clone();
        res.extend(ys.clone());
        Ok(Value::List(res))
      }
      (_, expr::BinaryOpTy::EqualEqual, _) => Ok(Value::Bool(Interpreter::equals(&lhs, &rhs))),
      (_, expr::BinaryOpTy::NotEqual, _) => Ok(Value::Bool(!Interpreter::equals(&lhs, &rhs))),
      _ => Err(format!(
        "invalid operands in binary operator {:?} of type {:?} and {:?} at line={},col={}",
        op.ty,
        type_of(&lhs),
        type_of(&rhs),
        op.line,
        op.col
      )),
    }
  }

  fn equals(lhs: &Value, rhs: &Value) -> bool {
    match (lhs, rhs) {
      (Value::Number(n1), Value::Number(n2)) => (n1 - n2).abs() < f64::EPSILON,
      (Value::String(s1), Value::String(s2)) => s1 == s2,
      (Value::Bool(b1), Value::Bool(b2)) => b1 == b2,
      (Value::Nil, Value::Nil) => true,
      (_, _) => false,
    }
  }

  fn interpret_unary(&mut self, op: expr::UnaryOp, expr: &expr::Expr) -> Result<Value, String> {
    let val = self.interpret_expr(expr)?;

    match (op.ty, &val) {
      (expr::UnaryOpTy::Minus, Value::Number(n)) => Ok(Value::Number(-n)),
      (expr::UnaryOpTy::Bang, _) => Ok(Value::Bool(!Interpreter::is_truthy(&val))),
      (_, Value::String(_)) => Err(format!(
        "invalid application of unary op {:?} to object of type String at line={},col={}",
        op.ty, op.line, op.col
      )),
      (_, Value::NativeFunction(_)) => Err(format!(
        "invalid application of unary op {:?} to object of type NativeFunction at line={},col={}",
        op.ty, op.line, op.col
      )),
      (_, Value::ChopinFunction(_, _, _)) => Err(format!(
        "invalid application of unary op {:?} to object of type ChopinFunction at line={},col={}",
        op.ty, op.line, op.col
      )),
      (_, Value::ChopinClass(_, _)) => Err(format!(
        "invalid application of unary op {:?} to object of type ChopinClass at line={},col={}",
        op.ty, op.line, op.col
      )),
      (_, Value::ChopinInstance(class_name, _)) => Err(format!(
        "invalid application of unary op {:?} to object of type {:?} at line={},col={}",
        class_name.name, op.ty, op.line, op.col
      )),
      (expr::UnaryOpTy::Minus, Value::Bool(_)) => Err(format!(
        "invalid application of unary op {:?} to object of type Bool at line={},col={}",
        op.ty, op.line, op.col
      )),
      (_, Value::Nil) => Err(format!(
        "invalid application of unary op {:?} to nil at line={},col={}",
        op.ty, op.line, op.col
      )),
      (_, Value::List(_)) => Err(format!(
        "invalid application of unary op {:?} to list at line={},col={}",
        op.ty, op.line, op.col
      )),
    }
  }

  fn is_truthy(val: &Value) -> bool {
    match val {
      Value::Nil => false,
      Value::Bool(b) => *b,
      _ => true,
    }
  }

  fn interpret_literal(lit: &expr::Literal) -> Value {
    match lit {
      expr::Literal::Number(n) => Value::Number(*n),
      expr::Literal::String(s) => Value::String(s.clone()),
      expr::Literal::True => Value::Bool(true),
      expr::Literal::False => Value::Bool(false),
      expr::Literal::Nil => Value::Nil,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::interpreter;
  use crate::parser;
  use crate::scanner;

  fn evaluate(code: &str) -> Result<String, String> {
    let tokens = scanner::scan_tokens(code.to_string()).unwrap();

    match parser::parse(tokens) {
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
  fn test_fact() {
    fn fact(n: i32) -> i32 {
      if n <= 1 {
        return 1;
      }
      return n * fact(n - 1);
    }

    check_output(
      "fun fact(n) { \n\
               if (n <= 1) {\n\
                   return 1; \n\
               }\n\
               return n * fact(n - 1); \n\
             } \n\
             print fact(10); ",
      &format!("{}", fact(10)),
    )
  }

  #[test]
  fn test_invalid_binary_operands() {
    check_error("1 + \"string\";", &|err: &str| {
      assert!(err.starts_with("invalid operands in binary operator"))
    })
  }

  #[test]
  fn test_invalid_unary_operand() {
    check_error("-\"cat\";", &|err: &str| {
      assert!(err.starts_with("invalid application of unary op Minus to object of type String"))
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
               return \"Hi, \" + first + \" \" + last + \"!\";\n\
             }\n\
             \n\
             print sayHi(\"Dear\", \"Reader\");\n\
             \n\
             fun add(x,y,z) {\n\
                 return x + y + z;\n\
             }\n\
             \n\
             print add(1,2,3);",
      "'Hi, Dear Reader!'\n6",
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
  fn test_implicit_return_nil() {
    check_output("fun f() {} print f();", "nil")
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
  fn test_classes_1() {
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
  fn test_classes_2() {
    check_output(
      "class DevonshireCream {\n\
               serveOn() {\n\
                 return \"Scones\";\n\
               }\n\
             }\n\
             \n\
             var inst = DevonshireCream();\n\
             print inst;",
      "ChopinInstance(DevonshireCream)",
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