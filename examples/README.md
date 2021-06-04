## Learn Chopin in y minutes

### 0. Hello World

```javascript
// This is simple comment
// To print something on your screen use print method 
print "Hello world"
```

### 1. Primitive Datatypes and Operators
#### 1.1 Numbers
```javascript
// You have integer numbers
2;

// and also float/decimal numbers
1.3;

// Math is what you would expect
2 + 1;    // 3
3 - 7;    // -4
1 * 2;    // 2

// The result of division is always a float
3 / 9;    // 0.3333333333333333

// Enforce precedence with parentheses
1 + 3 * 2;    // 7
(1 + 3) * 2;  // 8

// Boolean values are primitives
true;
false;

// Negate with !
!true;    // false

// Boolean Operators
true and true;   // true
true or false;   // true

// Equality is ==
1 == 1;         // true
1 == 2;         // false
true == true;   // true  

// Inequality is !=
1 != 2;         // true

// More comparisons
1 > 0;          // true
21 < 37;        // true
3 <= 2;         // false
1.1 >= 1;       // true

// Seeing whether a value is in a range
1 < 2 and 2 < 3;  // true
```

#### 1.2 Strings
```javascript
"This is a string";

// Get length of a string
len("hello world");  // 11
```

#### 1.3 None value
```javascript
// This is None value
nil;

1 == nil;        // false
"Hello" != nil;  // true
```

### 2. Variables and Collections

#### 2.1 Variables
```javascript
// To declare variable use var
var a = 1;
var b = 21.9;
print a + b;

// You can declare variable without assignment
var c;
c = 10;
```

#### 2.2 Arrays

```javascript
// You can start with a prefilled array
var l = [0,1];
print l;      // [0, 1]

// Access an array element like you would any other array
print "Index 0: " + l[0];
print "Index -1: " + l[-1];

// Arrays concatenation
var r = [1,2];
print l + r;    // [0, 1, 1, 2]

// Examine the length with "len()"
print len(r);   // 2
```

### 3. Control Flow and Iterables

#### 3.1 If statement
```javascript
// Let's just make a variable
var a = 5;

if (a < 10) {
    print "Hello";
} else {
    print "Hola";
}

// You can also make else if statement
if (a < 10) {
    print "Hello";
} else if (a == 10) {
    print "Hola";
} else {
    print "Ciao";
}
```

#### 3.2 Loops
```javascript
var tick = 0;

print "Odd number counting using for loop";

// Classic for loop
for (var i = 0; i < 10; i = i + 1) {
  if (tick == 1) {
    print i;
    tick = 0;
  } else {
    tick = tick + 1;
  }
}

// And while loop
var counter = 0;

while (counter < 10) {
  print counter;
  counter = counter + 1;
}

// You can create infinite loop like this
while (true) {
    // do something
}


// forEach loop
var arr = [1, 2, 3];
fun printer(item) {
    // do something
    print item;
}
forEach(arr, printer);
```

#### 4. Functions
```javascript
// Use "fun" to create new functions
fun foo() {
    print "Hello world";
}

// You can also create a function with arguments
fun add(x, y) {
    print x + y;
}

add(1, 2);

// Return values with a return statement
fun add2(x, y) {
    return x + y;
}

var result = add2(1, 1);

// Functions in Chopin are late binging
fun a() {
  b();
}

fun b() {
  print "hello world";
}

a();    // prints "hello world"
```

### 5. Classes
```javascript
// We use the "class" statement to create a class

class Animal {
    // class body
    
    //  An instance method
    foo() {
        print "This is an instance method";
    }
}

// Create an instance of Animal class
var animal = Animal();

// Call our class method
animal.foo();

// Inheritance allows new child classes to be defined that inherit methods and
// variables from their parent class.

class Human < Animal {
    bar() {
        print "bar method";
    }
    
    // The "super" function lets you access the parent class's methods
    humanFoo() {
        print "Human uses Animal's foo";
        super.foo();
    }
}

var human = Human();
human.foo();
human.bar();
human.humanFoo();
```