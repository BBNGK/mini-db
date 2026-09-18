# Query Language

## Motivation

Inspired by [Acadia](https://acadia.engineering/), this query language follows the functional programming paradigm for creating database queries. However, since this project is not tied to an SQL-dialect, this language can take some liberties on features and semantics that wouldn't normally be possible in SQL-dialects (without extensions and plugins of course) as it does not need to compile down to an SQL-dialect.

## Key Features

### Transaction Effect Tracking

There are two types of functions: a "pure" function and a `transactional` function. Similar to `async`, `transactional` functions can only be called inside other `transactional` functions. This allows for precise tracking of which functions will need to actually need to touch the database (as that is the only side effect in the query language currently) and which functions are pure for optimization purposes (such as building up memomizations). Furthermore, since now the database "effect" is tracked, we can tackle the 1+N query problem in a more fine-grain manner by only disabling recursion (and other unbounded execution) on `transactional` functions. This allows for "pure" (compute) functions to still have the capability of recursion.

### Custom Column Types and Pattern Matching

> [!NOTE]
> This feature is planned but not fully flushed out

The ability to define and store precise custom data types in tables and columns and use them in queries. Because this query language is not tied to an SQL-dialect, some features may be sensible to include. The ability to create columns of custom user-defined data types and being able to pattern match on them during query filtering is a promising idea to allow for the database also be more precisely typed (parse, don't validate).

## Syntax

```rust
mod Pokemon with (
    getPokemon as endpoint,
    addPokemon as endpoint,
)

import Rows
import Security
import Sequence as Seq
import Table

// define the table type
type Pokemon {
    id : PokemonID,
    name : String,
    level: UInt64,
}

// define a newtype for ids
type PokemonID = UInt64

// creation of the table
let pokemon = Table.table<Pokemon>(
    primary = { $0.id },
    security = Security.Unrestricted,
    indexes = [],
    constraints = [],
    triggers = [],
)

// infinite sequence of uint64 for the primary key
let pokemonIds = Seq.uint64 { PokemonID($0) }

// get pokemon, access the pokemon table and extract the name of each one
transactional fn getPokemon() -> Rows<String> {
    return pokemon
        .map { $0.name }
        .selectAll()
}

// computational ("pure") function which is allowed to have recursion
fn fib(n: UInt64) -> UInt64 {
    return if (n < 2) {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

// add a new pokemon, by name, and by a level which is an index into the fibonacci sequence
transactional fn addPokemon(name: String, level: UInt64) {
    let id = pokemonIds.next()

    foods
        .insert(Security.Unrestricted) {
            Pokemon(
                id,
                name,
                fib(level)
            )
        }
}
```

## Grammar

```ebnf
program = moduleDecl declaration* EOF ;

moduleDecl   = "mod" identifier ("with" "(" exposures ")")? ;
exposures    = exposure ("," exposure)* ","? ; 
exposure     = identifier "as" exposureType ;
exposureType = "endpoint" ;

declaration = typeDecl
            | importDecl
            | functionDecl
            | variableDecl ;

type  = identifier ("<" types ">")? | "transactional"? "fn" "(" types? ")" ("->" type)? ;
types = type ("," type)* ","? ;

typeDecl = "type" identifier "{" fields? "}" 
         | "=" identifier EOS ;
fields   = field ("," field)* ","? ;
field    = identifier ":" type ; 

importDecl = "import" identifier ("as" identifier)? ;

block            = "{" localDeclaration* "}" ;
localDeclaration = functionDecl 
                 | variableDecl 
                 | statement ;

statement  = expression EOS 
           | assignment 
           | "return" expression? EOS 
           | ifStmt
           | forStmt
           | whileStmt ;
assignment = identifier "=" expression EOS ;
ifStmt     = "if" "(" expression ")" block ("else" (block | statement))? ;
forStmt    = "for" "(" identifier "in" expression ")" block ;
whileStmt  = "while" "(" expression ")" block ;

functionDecl = "transactional"? "fn" identifier "(" parameters? ")" ("->" type)? block ;
parameters   = parameter ("," parameter)* ","? ;
parameter    = identifier ":" type ;

variableDecl = "let" identifier (":" type)? "=" expression EOS ;

expression  = logical_or ;
logical_or  = logical_and ("||" logical_and)* ;
logical_and = equality ("&&" equality)* ;
equality    = comparison (("!=" | "==") comparison)* ;
comparison  = term ((">" | ">=" | "<" | "<=" ) term)* ;
term        = factor (("-" | "+") factor)* ;
factor      = unary (("/" | "*" | "%") unary)* ;
unary       = ("!" | "-") unary | call ;
call        = primary (callArgs | "." identifier)* ;
callArgs    = ("<" types ">")? "(" arguments? ")" ("{" expression* "}")?
            | ("<" types ">")? "{" expression* "}" ;
arguments   = (identifier "=")? expression ("," (identifier "=")? expression)* ","? ;
primary     = "(" expression ")"
            | "[" (expression ("," expression)* ","?)? "]"
            | "{" expression* "}"
            | "true" 
            | "false"
            | "$" digit+
            | number
            | string
            | identifier ;

number     = digit+ ("." digit+)? ;
string     = "\"" <any character>* "\"" ;
identifier = alpha (alpha | digit)* ;
alpha      = "a" .. "z" | "A" .. "Z" | "_" ;
digit      = "0" .. "9" ;
EOS        = newline | ";" ;
```