# Rust coding guidelines

When engaging in collaborative software projects, it is crucial to ensure that the code is well-organized and comprehensible. This facilitates ease of maintenance and allows for seamless extension of the project. To accomplish this objective, it is essential to establish shared guidelines that the entire development team adheres to.

The goal is to get a harmonized code-base which appears to come from the same hands.
This simplifies reading and understanding the intention of the code and helps maintaining the development speed.

The following chapters describe rules and concepts to fit clean code expectations.

## Clean code

We like our code clean and thus use the "Clean Code" rules from "uncle Bob". A short summary can be found [here](https://gist.github.com/wojteklu/73c6914cc446146b8b533c0988cf8d29). 

As rust could get a bit messy, feel free to add some additional code comments to blocks that cannot be made readable using the clean code rules.

## Naming conventions

We follow the standard [Rust naming conventions](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md).

Names of components, classes , functions, etc. in code should also follow the prescriptions in SW design. Before thinking of new names, please make sure that we have not named the beast already.

Names of unit tests within a file shall be hierarchical. Tests which belong together shall have the same prefix. For example the file `workload.rs` contains following tests:

* `container_create_success`
* `container_create_failed`
* `container_start_success`
* `container_start_failure_no_id`
 
So if you want to call tests which work with container, you can write

```shell
cargo test container
```

If you want to call tests of the "container create" function, you can call:

```shell
cargo test container_create
```

More information about calling unit tests is in
[The Rust Programming Language](https://doc.rust-lang.org/book/ch11-02-running-tests.html).

## Logging conventions

The following chapters describe rules for creating log messages.

### Log format of internal objects

When writing log messages that reference internal objects, the objects shall be surrounded in single quotes, e.g.:

```rust
log::info!("This is about object '{}'.", object.name)
```

This helps differentiate static from dynamic data in the log message.

### Log format of multiline log messages

Multi line log messages shall be created with the `concat!` macro, e.g.:

```rust
log::debug!(concat!(
    "First line of a log message that lists something:\n",
    "   flowers are: '{}'\n",
    "   weather is: {}")
    color, current_weather);
```

This ensures that the log messages are formatted correctly and simplifies writing the message.

### Choose a suitable log severity

| Severity | Use Case |
| --- | --- |
| Trace | A log that is useful for diagnostic purposes and/or more granular than severity debug. |
| Debug | A log that is useful for developers meant for debugging purposes or hit very often. |
| Info | A log communicating important information like important states of an application suitable for any kind of user and that does not pollute the output. |
| Warn | A log communicating wrong preconditions or occurrences of something unexpected but do not lead to a panic of the application. |
| Error | A log communicating failures and consequences causing a potential panic of the application. |

## Unit test convenience rules

The following chapter describes important rules about how to write unit tests.

### Test mock/object generation

When writing tests, one of the most tedious task is to setup the environment and create the necessary objects and/or mocks to be able to test the desired functionality. Following the [DRY](https://en.wikipedia.org/wiki/Don%27t_repeat_yourself) principle and trying to save some effort, we shall always place the code that generates a test or mock object in the same module/file where the mock of the object is defined.

For example, when you would like to **generate and reuse** a mock for the `Directory` structure located in the `agent/src/control_interface/directory.rs` file, **you shall**

* write a public setup function:
  ```rust
  pub fn generate_test_directory_mock() -> __mock_MockDirectory::__new::Context;
  ```
  The `<datatype_name>` in `__mock_Mock<datatype_name>::__new::Context` must be replaced with the name of the type the mock is created for.
  
* place the function in the test part of the file (after the test banner if you use one)
* place a `#[cfg(test)]` (or `#[cfg(feature = "test_utils")]` in case of a library) before the function to restrict its compilation to test only
* use this function in all places where you need
* 
If you need some variation in the output or the behavior of the function, you can, of course, make it parametrized.

All **object/mock generation functions shall start** with `generate_test_`.

## Advanced rules

### Don' t reinvent the wheel

Bad:

```rust
let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8];

let mut filtered_numbers = Vec::new();
// filter numbers smaller then 3
for number in numbers {
    if number < 3 {
        filtered_numbers.push(number);
    }
}
```

Good:

Prefer standard library algorithms over own implementations to avoid error prone code.

```rust
let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8];
let filtered_numbers: Vec<i32> = numbers.into_iter().filter(|x| x < &3).collect();
```

### Prefer error propagation

Bad:

A lot of conditionals for opening and reading a file.

```rust
use std::fs::File;
use std::io;
use std::io::Read;

fn read_from_file(filepath: &str) -> Result<String, io::Error> {
    let file_handle = File::open(filepath);
    let mut file_handle = match file_handle {
        Ok(file) => file,
        Err(e) => return Err(e),
    };
    
    let mut buffer = String::new();
    
    match file_handle.read_to_string(&mut buffer) {
        Ok(_) => Ok(buffer),
        Err(e) => Err(e)
    }
}
```

Good:

Prefer error propagation over exhaustive match and conditionals.

Error propagation shortens and cleans up the code path by replacing complex and exhaustive conditionals 
with the `?` operator without loosing the failure checks.

The refactored variant populates the error and success case the same way to the caller like in the bad example above,
but is more readable:
```rust
fn read_from_file(filepath: &str) -> Result<String, io::Error> {
    let mut buffer = String::new();
    File::open(filepath)?.read_to_string(&mut buffer)?;
    Ok(buffer)
}
```

In case of mismatching error types, provide a custom [From-Trait](https://doc.rust-lang.org/rust-by-example/conversion/from_into.html) implementation 
to convert between error types to keep the benefits of using the `?` operator. 
But keep in mind that error conversion shall be used wisely
(e.g. for abstracting third party library error types or if there is a benefit to introduce a common and reusable error type). 
The code base shall not be spammed with From-Trait implementations to replace each single match or conditional.

Error propagation shall also be preferred when converting between `Result<T,E>` and `Option<T>`.

Bad: 

```rust
fn string_to_percentage(string: &str) -> Option<f32> {
    // more error handling
    match string.parse::<f32>() {
        Ok(value) => Some(value * 100.),
        _ => None,
    }
}
```

Good:

```rust
fn string_to_percentage(string: &str) -> Option<f32> {
    // more error handling
    let value = string.parse::<f32>().ok()?; // returns None on parsing error
    Some(value * 100.)
}

```

### Avoid unwrap and expect

`Unwrap` or `expect` return the value in success case or call the `panic!` macro if the operation has failed.
Applications that are often terminated directly in case of errors are considered as unprofessional and not useful.

Bad:
```rust
let value = division(10, 0).unwrap(); // panics, because of a simple division!!!
```

Good:

Replace `unwrap` or `expect` with a conditional check, e.g. match expression:

```rust
let value = division(10, 0); // division 10 / 0 not allowed, returns Err

// conditional check before accessing the value
match value {
    Ok(value) => println!("{value}"),
    Err(e) => eprintln!("{e}")
}
```

or with if-let condition when match is awkward:

```rust
// access value only on success
if let Ok(value) = division(10, 0) {
    println!("{value}")
}
```

or if possible continue with some default value in case of an error:

```rust
let result = division(10, 0).unwrap_or(0.);
```

Exceptions:

In some cases terminating a program might be necessary. To make a good decision when to panic a program or not, the official rust book might help: [To panic! or Not to panic!](https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html)

When writing unit tests using `unwrap` helps to keep tests short and to concentrate on the `assert!` statements:

Bad:

```rust
let container: Option<HashMap<i32, String>> = operation_under_test();
match container {
    Some(container) => {
        match container.get(&0) {
            Some(value_of_0) => assert_eq!(value_of_0, &"hello world".to_string()),
            _ => { panic!("Test xy failed, no entry.") }
        }
    },
    _ => { panic!("Test xy failed, no container.") }
}
```

Good:

Prefer direct `unwrap` calls over `assert!` statements nested in complex conditional clauses.
It is shorter and the `assert!` statement is directly eye-catching.

```rust
let container: Option<HashMap<i32, String>> = operation_under_test();
let value_of_0 = container.unwrap().remove(&0).unwrap(); // the test is failing on error

assert_eq!(value_of_0, "hello world".to_string());
```

### Prefer while-let over match in loops

Use the shorter and cleaner while-let expression to eliminate exhaustive match sequences in loops:

Bad:

```rust
loop {
    match generate() {
        Some(value) => println!("{value}"),
        _ => { break; },
    }
}

```

Good:

```rust
// if success use the value else break
// ...or while let Ok(value) in case of Result<T,E> instead of Option<T>
while let Some(value) = generate() {
    println!("{value}")
}
```

### Prefer lazily evaluated functional chaining

Bad:

Eagerly evaluated functions are always evaluated regardless of the success or error case.
If the alternative is not taken potentially costly operations are performed unnecessarily.

```rust
let value = division(2., 10.);
let result = value.and(to_percentage(value)); // eagerly evaluated

let value = division(2., 10.);
let result = value.or(provide_complex_alternative()); // eagerly evaluated

let value = division(2., 10.);
let result = value.unwrap_or(generate_complex_default()); // eagerly evaluated
```

Good:

Lazily evaluated functions are only evaluated if the case actually occurs and
are preferred if the alternatives provide costly operations.

```rust
let result = division(2., 10.).and_then(to_percentage); // lazily evaluated

let result = division(2., 10.).or_else(provide_complex_alternative); // lazily evaluated

let result = division(2., 10.).unwrap_or_else(generate_complex_default); // lazily evaluated
```

### Avoid exhaustive nested code

Bad:

The code is hard to read and the interesting code path is not an eye-catcher.

```rust
fn list_books(&self) -> Option<Vec<String>> {
    if self.wifi {
        if self.login {
            if self.admin {
                return Some(get_list_of_books());
            } else {
                eprintln!("Expected login as admin.");
            }
        } else {
            eprintln!("Expected login.");
        }
    } else {
        eprintln!("Expected connection.");
    }
    None
}
```

Good:

Nest code only into 1 or 2 levels.
Use early-exit pattern to reduce the nest level and to separate error handling code from code doing the actual logic.

```rust
fn list_books(&self) -> Option<Vec<String>> {
    if !self.wifi {
        eprintln!("Expected connection.");
        return None;
    }

    if !self.login {
        eprintln!("Expected login.");
        return None;
    }

    if !self.admin {
        eprintln!("Expected login as admin.");
        return None;
    } 

    // interesting part
    Some(get_list_of_books())
}
```

As an alternative, when dealing with `Option<T>` or `Result<T,E>` use Rust's powerful [combinators](https://doc.rust-lang.org/rust-by-example/error/option_unwrap/map.html) to keep the code readable.

### Follow common Rust principles and idioms

Understanding and practicing important Rust idioms help to write code in an idiomatic way,
meaning resolving a task by following the conventions of a given language.
Writing idiomatic Rust code ensures a clean and consistent code base.
Thus, please follow the guidelines of [Idiomatic Rust](https://github.com/mre/idiomatic-rust).

### Avoid common anti-patterns

There are a lot of Rust anti-patterns that shall not be used in general.
To get more details about anti-patterns, see [here](https://rust-unofficial.github.io/patterns/anti_patterns/index.html).

### Don't make sync code async

Async code is mainly used for I/O intensive, network or background tasks (Databases, Servers) to allow executing such tasks in a non-blocking way,
so that waiting times can be used reasonably for executing other operations.
However operations that do not fit to async use cases and are called synchronously shall not be made async because there is no real benefit.
Async code is more difficult to understand than synchronous code.

Bad:

No need for making those operations async, because they are exclusively called synchronously.
It is just more syntax and the code raises more questions about the intent to the reader.
```rust
let result1 = operation1().await;
let result2 = operation2().await;
let result3 = operation3().await;
```

Good:

Keep it synchronous and thus simple.
```rust
let result1 = operation1();
let result2 = operation2();
let result3 = operation3();
```

### Don’t mix sync and async code without proper consideration

Mixing sync and async code can lead to a number of problems, including performance issues, deadlocks, and race conditions.
Avoid mixing async with sync code unless there is a good reason to do so.

# Further Readings

* <https://rustc-dev-guide.rust-lang.org/conventions.html>
* <https://www.kernel.org/doc/html/next/rust/coding-guidelines.html>
* <https://rust-lang.github.io/api-guidelines/about.html>
